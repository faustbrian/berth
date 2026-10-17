//! cli support resolve tty module.
//!
//! Contains cli support resolve tty logic used by Berth command workflows.

/// Resolves tty using configured inputs and runtime state.
pub(crate) fn resolve_tty(tty: bool, no_tty: bool) -> bool {
    if no_tty {
        return false;
    }

    let attached_terminal = has_attached_terminal();
    if !attached_terminal {
        return false;
    }

    if tty {
        return true;
    }

    attached_terminal
}

pub(crate) fn effective_tty(tty: bool, no_tty: bool) -> bool {
    resolve_tty(tty, no_tty)
}

fn has_attached_terminal() -> bool {
    use std::io::IsTerminal;

    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::{has_attached_terminal, resolve_tty};

    #[test]
    fn no_tty_takes_precedence_over_tty_flag() {
        assert!(!resolve_tty(true, true));
    }

    #[test]
    fn tty_flag_still_requires_attached_terminal() {
        assert_eq!(resolve_tty(true, false), has_attached_terminal());
    }
}
