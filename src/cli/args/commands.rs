//! cli args commands module.
//!
//! Contains cli args commands logic used by Berth command workflows.

use clap::Subcommand;

mod app;
mod daemon;
mod lifecycle;
mod meta;
mod operations;

#[cfg(test)]
pub(crate) use app::TaskDepsArgs;
pub(crate) use app::{
    AppCreateArgs, ArtisanArgs, BunArgs, ComposerArgs, DenoArgs, EnvScrubArgs, ExecArgs, NodeArgs,
    OpenArgs, PhpToolArgs, ServeArgs, ShareArgs, ShareCommands, ShareProviderSelectionArgs,
    TaskArgs, TaskCommands, TaskDepsCommands,
};
pub(crate) use daemon::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonRunArgs, DaemonServiceArgs,
    DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs, DaemonStartArgs,
    DaemonStatusArgs, DaemonStopArgs, DaemonWatchArgs, DaemonWatchPolicyArgs,
};

#[cfg(test)]
pub(crate) use app::{ShareStartArgs, ShareStatusArgs, ShareStopArgs};

pub(crate) use lifecycle::{
    ApplyArgs, DownArgs, RecreateArgs, RelabelArgs, RestartArgs, RmArgs, SetupArgs, StartArgs,
    StopArgs, UpArgs, UpdateArgs, UrlArgs,
};

pub(crate) use meta::{CompletionsArgs, ConfigArgs, DoctorArgs, LockArgs, PresetArgs, ProfileArgs};

pub(crate) use operations::{
    AboutArgs, AttachArgs, CpArgs, DumpArgs, EnvArgs, EventsArgs, GcArgs, HealthArgs, InspectArgs,
    KillArgs, LogsArgs, LsArgs, PauseArgs, PortArgs, PruneArgs, PsArgs, PullArgs, RestoreArgs,
    StatsArgs, SwarmArgs, TopArgs, UnpauseArgs, WaitArgs,
};

#[derive(Subcommand)]
#[non_exhaustive]
pub(crate) enum Commands {
    /// Initialize a new .berth.yaml project file
    Init,
    /// Print resolved configuration
    Config(ConfigArgs),
    /// Inspect available service presets
    Preset(PresetArgs),
    /// Inspect profile groupings
    Profile(ProfileArgs),
    /// Validate local setup and config health
    Doctor(DoctorArgs),
    /// Manage workspace lockfile for reproducible image resolution
    Lock(LockArgs),
    /// Optional file-watching daemon for humans; no command needs it
    Daemon(DaemonArgs),
    /// Prepare service(s)
    Setup(SetupArgs),
    /// Run doctor, start services, bootstrap app runtime, then open app URLs
    Start(StartArgs),
    /// Create or converge this checkout's instance (`--wait --json` for agents)
    Up(UpArgs),
    /// Converge services and apply configured data seeds
    Apply(ApplyArgs),
    /// Pull latest images and restart selected services
    Update(UpdateArgs),
    /// Remove the instance's containers (`--purge` also deletes its data)
    Down(DownArgs),
    /// Stop service container(s)
    Stop(StopArgs),
    /// Remove service container(s)
    Rm(RmArgs),
    /// Destroy and recreate service container(s) from scratch
    Recreate(RecreateArgs),
    /// Restart service container(s)
    Restart(RestartArgs),
    /// Recreate containers to apply current Berth ownership labels
    Relabel(RelabelArgs),
    /// Print connection URL(s)
    Url(UrlArgs),
    /// Restore a SQL file into a database service
    Restore(RestoreArgs),
    /// Dump a database service to a SQL file
    Dump(DumpArgs),
    /// List service runtime status
    #[command(visible_alias = "status")]
    Ps(PsArgs),
    /// Show project runtime overview
    About(AboutArgs),
    /// Check if service(s) are ready to accept connections
    Health(HealthArgs),
    /// Print the managed environment containers receive (`.env` is never rewritten)
    Env(EnvArgs),
    /// Show container logs
    Logs(LogsArgs),
    /// Show running processes in container(s)
    Top(TopArgs),
    /// Show a live stream of container resource usage
    Stats(StatsArgs),
    /// Show low-level details for container(s)
    Inspect(InspectArgs),
    /// Attach local standard input/output/error streams to a running container
    Attach(AttachArgs),
    /// Copy files/folders between host and container
    Cp(CpArgs),
    /// Force-stop running container(s)
    Kill(KillArgs),
    /// Pause all processes in container(s)
    Pause(PauseArgs),
    /// Unpause all processes in container(s)
    Unpause(UnpauseArgs),
    /// Block until container(s) stop and print exit status
    Wait(WaitArgs),
    /// Stream container runtime events (Berth scope by default)
    Events(EventsArgs),
    /// List port mappings for container(s)
    Port(PortArgs),
    /// Remove stopped Berth service containers (or all with --all)
    Prune(PruneArgs),
    /// Pull latest service image(s)
    Pull(PullArgs),
    /// Run a command inside a service container
    Exec(ExecArgs),
    /// Bootstrap Laravel app runtime (key, cache clear, migrate, storage link)
    AppCreate(AppCreateArgs),
    /// Run php artisan inside a serve container
    Artisan(ArtisanArgs),
    /// Run composer inside an app container
    Composer(ComposerArgs),
    /// Run phpstan inside an app container
    Phpstan(PhpToolArgs),
    /// Run ECS inside an app container
    Ecs(PhpToolArgs),
    /// Run php-cs-fixer inside an app container
    PhpCsFixer(PhpToolArgs),
    /// Run Psalm inside an app container
    Psalm(PhpToolArgs),
    /// Run Pint inside an app container
    Pint(PhpToolArgs),
    /// Run Pest inside an app container
    Pest(PhpToolArgs),
    /// Run PHPUnit inside an app container
    Phpunit(PhpToolArgs),
    /// Run Rector inside an app container
    Rector(PhpToolArgs),
    /// Run Node package manager commands inside an app container
    Node(NodeArgs),
    /// Run Bun inside an app container
    Bun(BunArgs),
    /// Run Deno inside an app container
    Deno(DenoArgs),
    /// Run opinionated internal task workflows
    Task(TaskArgs),
    /// List every instance this installation owns, across projects
    /// (`--services` lists the current project's services)
    Ls(LsArgs),
    /// Reap expired and orphaned instances, then remove unused shared servers
    Gc(GcArgs),
    /// Run a berth command across configured swarm targets
    Swarm(SwarmArgs),
    /// Generate shell completions
    Completions(CompletionsArgs),
    /// Start an app serve target and publish its gateway route
    Serve(ServeArgs),
    /// Print/open serve URL and show app/database health summary
    Open(OpenArgs),
    /// Share an app service through an external tunnel provider
    Share(ShareArgs),
    /// Scrub sensitive .env values with safe local placeholders
    EnvScrub(EnvScrubArgs),
}
