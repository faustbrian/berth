//! Where psql ends each statement of a dump: at a `;` outside quotes,
//! dollar-quoted bodies, comments, parentheses and `BEGIN … END` bodies, as
//! its own lexer finds it (PostgreSQL `src/fe_utils/psqlscan.l`).

/// What one line held, judged as psql reads it.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Line {
    /// It began outside any statement, quote or comment.
    began_clear: bool,
    /// Statements that ended on it.
    ends: usize,
    /// It holds a psql command (`\…`).
    command: bool,
    /// It ended outside any statement, quote or comment.
    ended_clear: bool,
    /// The statement that ended on it ends `OWNER TO <role>`, outside any
    /// quote, with no other subcommand (`, …`) before it.
    owner_change: bool,
    /// The statement that ended on it is a `COPY … FROM stdin`: psql reads
    /// its data from the lines after it.
    copy_from_stdin: bool,
    /// The dump did something this lexer cannot follow, on this line or
    /// before it: where statements end is no longer known.
    lost: bool,
    /// It holds a `\connect`, which moves the rest of the dump into another
    /// database.
    connects: bool,
}

impl Line {
    /// It holds exactly one statement, begun and ended on it: an error psql
    /// names by this line is that statement's.
    pub(super) fn single_statement(self) -> bool {
        self.began_clear && self.ended_clear && self.ends == 1 && !self.command && !self.lost
    }

    /// It began outside any statement, quote or comment: what it says is
    /// the dump's own, not a literal's.
    pub(super) fn began_clear(self) -> bool {
        self.began_clear && !self.lost
    }

    pub(super) fn owner_change(self) -> bool {
        self.owner_change
    }

    pub(super) fn copy_from_stdin(self) -> bool {
        self.copy_from_stdin
    }

    pub(super) fn connects(self) -> bool {
        self.connects
    }
}

/// What followed a closing quote.
#[derive(Debug, Clone, Copy)]
struct AfterQuote {
    /// It closed an escape string (`E'…'`).
    escapes: bool,
    /// Whitespace, and a newline in it, came since.
    spaced: bool,
    newline: bool,
}

/// A token of code, as far as the end of an owner change needs it.
#[derive(Debug, Clone, Copy, Default)]
enum Token {
    #[default]
    None,
    Word(Run),
    /// A quoted name (`"…"`).
    Name,
    Other,
}

#[derive(Debug)]
enum State {
    Code,
    /// `'…'`; `escapes` when a backslash escapes the next byte (`E'…'`).
    /// `backslash` when an odd run of backslashes ends here in a plain
    /// string, which a `'` next closes or not depending on psql's reading.
    Quoted {
        escapes: bool,
        escaped: bool,
        backslash: bool,
    },
    /// `"…"`.
    Identifier,
    /// After a `$`: the tag of a dollar quote, if a `$` closes it.
    DollarTag(Vec<u8>),
    /// Inside `$tag$…$tag$`; `matched` bytes of the closing one seen.
    Dollar {
        delimiter: Vec<u8>,
        matched: usize,
    },
    LineComment,
    /// `/* … */`, which nest.
    BlockComment {
        depth: usize,
        last: Option<u8>,
    },
    /// A psql command, to the end of its line, and its name (its arguments,
    /// which psqlscanslash.l lexes, may hand the line back to SQL at `\\`).
    Command {
        name: Vec<u8>,
        naming: bool,
    },
}

/// A run of name or number bytes in code.
#[derive(Debug, Clone, Copy)]
struct Run {
    /// It starts as a name does, not as a number.
    word: bool,
    /// Its first bytes, lowercased: enough to tell the words psql watches.
    head: [u8; Run::KEPT],
    len: usize,
    /// An odd run of `:` came right before it: psql puts a variable of
    /// that name in its place.
    after_colon: bool,
}

impl Run {
    const KEPT: usize = 9;

    fn is(&self, word: &[u8]) -> bool {
        self.word && self.len == word.len() && self.head.get(..self.len) == Some(word)
    }

    fn is_prefix(&self, start: &[u8]) -> bool {
        self.word && self.head.starts_with(start)
    }
}

#[derive(Debug)]
pub(super) struct Lexer {
    state: State,
    /// A `-` or `/` that opens a comment if the next byte is `-` or `*`,
    /// or a `\\` that the next byte makes a command or a literal `;`/`:`.
    held: Option<u8>,
    /// The last three tokens of the current statement.
    recent: [Token; 3],
    lost: bool,
    run: Option<Run>,
    /// A string just closed, and what came after it so far: a quote right
    /// after it, or after whitespace holding a newline, continues it
    /// (psqlscan.l's `xqs` and `quotecontinue`), escapes and all.
    after_quote: Option<AfterQuote>,
    /// Names since the statement began, and the first letters of its first
    /// four when they are `create`, `or`, `replace`, `function` or
    /// `procedure`: psql watches `BEGIN … END` only in a routine's
    /// definition.
    words: usize,
    marks: [u8; 4],
    /// The statement's first name, and whether `stdin` followed it.
    first: Option<Run>,
    stdin: bool,
    /// Commas outside parentheses since the statement began.
    commas: usize,
    parens: usize,
    begins: usize,
    /// A statement has begun and not ended.
    open: bool,
    /// `:` bytes of code just before this one.
    colons: usize,
    line: Line,
}

impl Lexer {
    pub(super) fn new() -> Self {
        Self {
            state: State::Code,
            held: None,
            recent: [Token::None; 3],
            lost: false,
            run: None,
            after_quote: None,
            words: 0,
            marks: [0; 4],
            first: None,
            stdin: false,
            commas: 0,
            parens: 0,
            begins: 0,
            open: false,
            colons: 0,
            line: Line {
                began_clear: true,
                ..Line::default()
            },
        }
    }

    /// From here on, where statements end is not known: nothing more is
    /// excused.
    pub(super) fn lose_track(&mut self) {
        self.lost = true;
    }

    pub(super) fn lost(&self) -> bool {
        self.lost
    }

    /// Ends the current line, saying what it held.
    pub(super) fn end_line(&mut self) -> Line {
        self.feed(b'\n');
        self.line.lost = self.lost;
        let clear = !self.open && matches!(self.state, State::Code);
        let mut line = std::mem::replace(
            &mut self.line,
            Line {
                began_clear: clear,
                ..Line::default()
            },
        );
        line.ended_clear = clear;
        line
    }

    /// Takes the next byte of a line (its newline goes through
    /// [`Self::end_line`]).
    pub(super) fn feed(&mut self, byte: u8) {
        if let Some(held) = self.held.take() {
            match (held, byte) {
                (b'-', b'-') => {
                    // A `--` comment continues a string only past a newline.
                    if !self.after_quote.is_some_and(|after| after.newline) {
                        self.after_quote = None;
                    }
                    self.state = State::LineComment;
                    return;
                }
                (b'/', b'*') => {
                    self.after_quote = None;
                    self.state = State::BlockComment {
                        depth: 1,
                        last: None,
                    };
                    return;
                }
                // psqlscan.l's `"\\"[;:]`: the character goes into the query
                // and lexing goes on.
                (b'\\', b';' | b':') => {
                    // A `\;` joins two statements into one query, which no
                    // line names alone; pg_dump writes neither.
                    self.lost = true;
                    self.push(Token::Other);
                    self.open = true;
                    return;
                }
                (b'\\', _) => {
                    self.state = State::Command {
                        name: Vec::new(),
                        naming: true,
                    };
                    self.line.command = true;
                }
                _ => self.code(held),
            }
        }
        match &mut self.state {
            State::Code => {
                if matches!(byte, b'-' | b'/' | b'\\') {
                    self.end_run();
                    self.held = Some(byte);
                } else {
                    self.code(byte);
                }
            }
            State::Quoted {
                escapes,
                escaped,
                backslash,
            } => {
                if *escaped {
                    *escaped = false;
                } else if *escapes && byte == b'\\' {
                    *escaped = true;
                } else if byte == b'\\' {
                    *backslash = !*backslash;
                } else if byte == b'\'' {
                    // psql reads a plain string as the server's
                    // `standard_conforming_strings` says, which a dump can
                    // turn off (`SET`, or a function), or the database may
                    // have off: then `\'` escapes the quote. Only there do
                    // the two readings part, so a dump that may be read
                    // either way is no longer followed.
                    if std::mem::take(backslash) {
                        self.lost = true;
                    }
                    self.after_quote = Some(AfterQuote {
                        escapes: *escapes,
                        spaced: false,
                        newline: false,
                    });
                    self.state = State::Code;
                    self.push(Token::Other);
                } else {
                    *backslash = false;
                }
            }
            State::Identifier => {
                if byte == b'"' {
                    self.state = State::Code;
                    self.push(Token::Name);
                }
            }
            State::DollarTag(tag) => {
                if byte == b'$' {
                    let mut delimiter = Vec::with_capacity(tag.len() + 2);
                    delimiter.push(b'$');
                    delimiter.append(tag);
                    delimiter.push(b'$');
                    self.state = State::Dollar {
                        delimiter,
                        matched: 0,
                    };
                } else if is_name_start(byte) || (!tag.is_empty() && byte.is_ascii_digit()) {
                    tag.push(byte);
                } else {
                    // `$1`, or a `$` that opens nothing: plain code.
                    self.state = State::Code;
                    self.feed(byte);
                }
            }
            State::Dollar { delimiter, matched } => {
                if delimiter.get(*matched) == Some(&byte) {
                    *matched += 1;
                    if *matched == delimiter.len() {
                        self.state = State::Code;
                        self.push(Token::Other);
                    }
                } else {
                    // A tag holds no `$`: a `$` here may open the closing one.
                    *matched = usize::from(byte == b'$');
                }
            }
            State::LineComment => {
                if byte == b'\n' {
                    if let Some(after) = &mut self.after_quote {
                        after.newline = true;
                    }
                    self.state = State::Code;
                }
            }
            State::Command { name, naming } => {
                if byte == b'\n' {
                    // pg_dump writes only these (`\restrict` and
                    // `\unrestrict` since 17.6; `\connect` with `--create`,
                    // which the restore refuses). Another (`\copy … from
                    // stdin` reads the lines after it as data; `\i` runs
                    // another file) is not followed.
                    if matches!(name.as_slice(), b"connect" | b"c") {
                        self.line.connects = true;
                    }
                    if !matches!(name.as_slice(), b"restrict" | b"unrestrict") {
                        self.lost = true;
                    }
                    self.state = State::Code;
                } else if *naming && !byte.is_ascii_whitespace() && byte != b'\\' {
                    if name.len() < 16 {
                        name.push(byte);
                    }
                } else if byte.is_ascii_whitespace() {
                    *naming = false;
                } else if byte == b'\\' {
                    // A second command, or `\\` handing the rest of the line
                    // back to SQL, past arguments quoted as psqlscanslash.l
                    // quotes them: not followed here. pg_dump writes
                    // neither (`\restrict`, `\connect`).
                    self.lost = true;
                }
            }
            State::BlockComment { depth, last } => match (last.take(), byte) {
                (Some(b'/'), b'*') => *depth += 1,
                (Some(b'*'), b'/') => {
                    *depth -= 1;
                    if *depth == 0 {
                        self.state = State::Code;
                    }
                }
                _ => *last = Some(byte),
            },
        }
    }

    fn code(&mut self, byte: u8) {
        let colons = self.colons;
        self.colons = if byte == b':' { colons + 1 } else { 0 };
        let after_quote = self.after_quote.take();
        if byte.is_ascii_whitespace() {
            self.after_quote = after_quote.map(|after| AfterQuote {
                spaced: true,
                newline: after.newline || byte == b'\n',
                ..after
            });
        }
        let continues_escapes =
            after_quote.is_some_and(|after| after.escapes && (!after.spaced || after.newline));
        if is_name_part(byte) && !(byte == b'$' && self.run.is_none()) {
            let run = self.run.get_or_insert(Run {
                word: is_name_start(byte),
                head: [0; Run::KEPT],
                len: 0,
                // psql reads `::` as a cast first, so the last of an odd
                // run (`:x`, `:::x`) still names a variable.
                after_colon: colons % 2 == 1,
            });
            if let Some(kept) = run.head.get_mut(run.len) {
                *kept = byte.to_ascii_lowercase();
            }
            run.len += 1;
            self.open = true;
            return;
        }
        if byte == b'\'' {
            // `E'…'` takes backslash escapes; `B'…'`, `X'…'`, `N'…'` are
            // strings too, not names.
            let run = self.run.take();
            let prefix = |letter: &[u8]| run.as_ref().is_some_and(|run| run.is(letter));
            let escapes = continues_escapes || prefix(b"e");
            let string_prefix = prefix(b"e") || prefix(b"b") || prefix(b"x") || prefix(b"n");
            if let Some(run) = run.filter(|_| !string_prefix) {
                self.count(run);
            }
            self.state = State::Quoted {
                escapes,
                escaped: false,
                backslash: false,
            };
            self.open = true;
            return;
        }
        self.end_run();
        match byte {
            b';' => {
                if self.parens == 0 && self.begins == 0 {
                    self.line.owner_change = self.commas == 0
                        && matches!(
                            self.recent,
                            [Token::Word(owner), Token::Word(to), Token::Word(_) | Token::Name]
                                if owner.is(b"owner") && to.is(b"to")
                        );
                    self.line.copy_from_stdin |=
                        self.stdin && self.first.is_some_and(|first| first.is(b"copy"));
                    self.recent = [Token::None; 3];
                    self.first = None;
                    self.stdin = false;
                    self.commas = 0;
                    self.open = false;
                    self.words = 0;
                    self.line.ends += 1;
                } else {
                    self.push(Token::Other);
                    self.open = true;
                }
            }
            b'"' => {
                self.state = State::Identifier;
                self.open = true;
            }
            b'$' => {
                self.state = State::DollarTag(Vec::new());
                self.open = true;
            }
            b'(' => {
                self.parens += 1;
                self.push(Token::Other);
                self.open = true;
            }
            b')' => {
                self.parens = self.parens.saturating_sub(1);
                self.push(Token::Other);
                self.open = true;
            }
            _ if byte.is_ascii_whitespace() => {}
            _ => {
                if byte == b',' && self.parens == 0 {
                    self.commas += 1;
                }
                self.push(Token::Other);
                self.open = true;
            }
        }
    }

    fn push(&mut self, token: Token) {
        self.recent.rotate_left(1);
        if let Some(last) = self.recent.last_mut() {
            *last = token;
        }
    }

    fn end_run(&mut self) {
        if let Some(run) = self.run.take() {
            self.count(run);
        }
    }

    /// psql's heuristic for semicolons inside a routine's `BEGIN ATOMIC …
    /// END` body (its `{identifier}` rule).
    fn count(&mut self, run: Run) {
        // `:LAST_ERROR_MESSAGE` puts the last error's text, which the dump
        // chose (`RAISE`), into the line, where psql lexes it as the dump's
        // own: it may close a quote or hold a psql command (`\set …`).
        // Every other variable psql sets holds berth's or the server's
        // words, and a dump that sets one is not followed past that command.
        if run.after_colon && run.len == "LAST_ERROR_MESSAGE".len() && run.is_prefix(b"last_erro") {
            self.lost = true;
        }
        // `FROM stdin` (or `pstdin`) at the top level: a column or alias
        // named `stdin` elsewhere in a `COPY (…) TO stdout` reads no data.
        let after_from =
            matches!(self.recent[2], Token::Word(from) if from.is(b"from")) && self.parens == 0;
        self.push(if run.word {
            Token::Word(run)
        } else {
            Token::Other
        });
        if !run.word {
            return;
        }
        if self.words == 0 {
            self.marks = [0; 4];
            self.first = Some(run);
        } else if after_from && (run.is(b"stdin") || run.is(b"pstdin")) {
            self.stdin = true;
        }
        let watched = [&b"create"[..], b"function", b"procedure", b"or", b"replace"];
        if let Some(mark) = self.marks.get_mut(self.words)
            && watched.iter().any(|word| run.is(word))
        {
            *mark = run.head.first().copied().unwrap_or_default();
        }
        self.words += 1;
        let [first, second, third, fourth] = self.marks;
        let routine = first == b'c'
            && (second == b'f'
                || second == b'p'
                || (second == b'o' && third == b'r' && (fourth == b'f' || fourth == b'p')));
        if !routine || self.parens != 0 {
            return;
        }
        if run.is(b"begin") {
            self.begins += 1;
        } else if run.is(b"case") {
            // `CASE` also ends with `END`; it matters only inside a body.
            if self.begins >= 1 {
                self.begins += 1;
            }
        } else if run.is(b"end") {
            self.begins = self.begins.saturating_sub(1);
        }
    }
}

/// psql's `ident_start`: a letter, `_`, or any byte of a multibyte character.
fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// psql's `ident_cont`, digits and `$` included.
fn is_name_part(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || byte == b'$'
}
