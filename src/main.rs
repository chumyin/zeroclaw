#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_possible_wrap,
    clippy::doc_markdown,
    clippy::field_reassign_with_default,
    clippy::float_cmp,
    clippy::implicit_clone,
    clippy::items_after_statements,
    clippy::map_unwrap_or,
    clippy::manual_let_else,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unused_self,
    clippy::cast_precision_loss,
    clippy::unnecessary_cast,
    clippy::unnecessary_lazy_evaluations,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_map_or,
    clippy::unnecessary_wraps,
    dead_code
)]

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use dialoguer::{Confirm, Input, Password};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{IsTerminal, Write};
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

fn parse_temperature(s: &str) -> std::result::Result<f64, String> {
    let t: f64 = s.parse().map_err(|e| format!("{e}"))?;
    if !(0.0..=2.0).contains(&t) {
        return Err("temperature must be between 0.0 and 2.0".to_string());
    }
    Ok(t)
}

mod agent;
mod approval;
mod auth;
mod channels;
mod rag {
    pub use zeroclaw::rag::*;
}
mod config;
mod cost;
mod cron;
mod daemon;
mod doctor;
mod gateway;
mod hardware;
mod health;
mod heartbeat;
mod hooks;
mod identity;
mod integrations;
mod memory;
mod migration;
mod multimodal;
mod observability;
mod onboard;
mod peripherals;
mod presets;
mod providers;
mod runtime;
mod security;
mod service;
mod skillforge;
mod skills;
mod tools;
mod tunnel;
mod updater;
mod util;

use config::Config;

// Re-export so binary modules can use crate::<CommandEnum> while keeping a single source of truth.
pub use zeroclaw::{
    ChannelCommands, CronCommands, HardwareCommands, IntegrationCommands, MigrateCommands,
    PeripheralCommands, ServiceCommands, SkillCommands,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    #[value(name = "bash")]
    Bash,
    #[value(name = "fish")]
    Fish,
    #[value(name = "zsh")]
    Zsh,
    #[value(name = "powershell")]
    PowerShell,
    #[value(name = "elvish")]
    Elvish,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum EstopLevelArg {
    #[value(name = "kill-all")]
    KillAll,
    #[value(name = "network-kill")]
    NetworkKill,
    #[value(name = "domain-block")]
    DomainBlock,
    #[value(name = "tool-freeze")]
    ToolFreeze,
}

/// `ZeroClaw` - Zero overhead. Zero compromise. 100% Rust.
#[derive(Parser, Debug)]
#[command(name = "zeroclaw")]
#[command(author = "theonlyhennygod")]
#[command(version)]
#[command(about = "The fastest, smallest AI assistant.", long_about = None)]
struct Cli {
    #[arg(long, global = true)]
    config_dir: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize your workspace and configuration
    Onboard {
        /// Run the full interactive wizard (default is quick setup)
        #[arg(long)]
        interactive: bool,

        /// Overwrite existing config without confirmation
        #[arg(long)]
        force: bool,

        /// Reconfigure channels only (fast repair flow)
        #[arg(long)]
        channels_only: bool,

        /// API key (used in quick mode, ignored with --interactive)
        #[arg(long)]
        api_key: Option<String>,

        /// Provider name (used in quick mode, default: openrouter)
        #[arg(long)]
        provider: Option<String>,
        /// Model ID override (used in quick mode)
        #[arg(long)]
        model: Option<String>,
        /// Memory backend (sqlite, lucid, markdown, none) - used in quick mode, default: sqlite
        #[arg(long)]
        memory: Option<String>,

        /// Official preset ID (used in quick mode, default: minimal)
        #[arg(long)]
        preset: Option<String>,

        /// Extra pack ID to add on top of selected preset (repeatable)
        #[arg(long = "pack")]
        pack: Vec<String>,

        /// Natural-language intent used to plan preset/packs in quick mode
        #[arg(long)]
        intent: Option<String>,

        /// Security profile for quick onboarding (default: strict)
        #[arg(long = "security-profile", value_enum)]
        security_profile: Option<SecurityProfileArg>,

        /// Confirm using a non-strict security profile in quick onboarding
        #[arg(long = "yes-security-risk")]
        yes_security_risk: bool,

        /// Preview quick onboarding plan without writing config/workspace files
        #[arg(long)]
        dry_run: bool,

        /// Emit structured JSON output (quick dry-run mode only)
        #[arg(long)]
        json: bool,

        /// Rebuild binary after onboarding with selected preset/packs
        #[arg(long)]
        rebuild: bool,

        /// Confirm rebuild execution
        #[arg(long)]
        yes_rebuild: bool,
    },

    /// Start the AI agent loop
    #[command(long_about = "\
Start the AI agent loop.

Launches an interactive chat session with the configured AI provider. \
Use --message for single-shot queries without entering interactive mode.

Examples:
  zeroclaw agent                              # interactive session
  zeroclaw agent -m \"Summarize today's logs\"  # single message
  zeroclaw agent -p anthropic --model claude-sonnet-4-20250514
  zeroclaw agent --peripheral nucleo-f401re:/dev/ttyACM0")]
    Agent {
        /// Single message mode (don't enter interactive mode)
        #[arg(short, long)]
        message: Option<String>,

        /// Provider to use (openrouter, anthropic, openai, openai-codex)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use
        #[arg(long)]
        model: Option<String>,

        /// Temperature (0.0 - 2.0)
        #[arg(short, long, default_value = "0.7", value_parser = parse_temperature)]
        temperature: f64,

        /// Attach a peripheral (board:path, e.g. nucleo-f401re:/dev/ttyACM0)
        #[arg(long)]
        peripheral: Vec<String>,
    },

    /// Check and apply `zeroclaw` binary updates from GitHub Releases
    Update {
        /// Apply update (default action is check-only when this flag is omitted)
        #[arg(long)]
        apply: bool,

        /// Specific release version to target (e.g. 0.1.0 or v0.1.0); defaults to latest release
        #[arg(long)]
        version: Option<String>,

        /// Install path for updated binary (defaults to the currently running executable path)
        #[arg(long)]
        install_path: Option<std::path::PathBuf>,

        /// Preview update steps without downloading/installing
        #[arg(long)]
        dry_run: bool,

        /// Confirm binary replacement for update apply
        #[arg(long)]
        yes: bool,
    },

    /// Start the gateway server (webhooks, websockets)
    #[command(long_about = "\
Start the gateway server (webhooks, websockets).

Runs the HTTP/WebSocket gateway that accepts incoming webhook events \
and WebSocket connections. Bind address defaults to the values in \
your config file (gateway.host / gateway.port).

Examples:
  zeroclaw gateway                  # use config defaults
  zeroclaw gateway -p 8080          # listen on port 8080
  zeroclaw gateway --host 0.0.0.0   # bind to all interfaces
  zeroclaw gateway -p 0             # random available port")]
    Gateway {
        /// Port to listen on (use 0 for random available port); defaults to config gateway.port
        #[arg(short, long)]
        port: Option<u16>,

        /// Host to bind to; defaults to config gateway.host
        #[arg(long)]
        host: Option<String>,
    },

    /// Start long-running autonomous runtime (gateway + channels + heartbeat + scheduler)
    #[command(long_about = "\
Start the long-running autonomous daemon.

Launches the full ZeroClaw runtime: gateway server, all configured \
channels (Telegram, Discord, Slack, etc.), heartbeat monitor, and \
the cron scheduler. This is the recommended way to run ZeroClaw in \
production or as an always-on assistant.

Use 'zeroclaw service install' to register the daemon as an OS \
service (systemd/launchd) for auto-start on boot.

Examples:
  zeroclaw daemon                   # use config defaults
  zeroclaw daemon -p 9090           # gateway on port 9090
  zeroclaw daemon --host 127.0.0.1  # localhost only")]
    Daemon {
        /// Port to listen on (use 0 for random available port); defaults to config gateway.port
        #[arg(short, long)]
        port: Option<u16>,

        /// Host to bind to; defaults to config gateway.host
        #[arg(long)]
        host: Option<String>,
    },

    /// Manage OS service lifecycle (launchd/systemd user service)
    Service {
        /// Init system to use: auto (detect), systemd, or openrc
        #[arg(long, default_value = "auto", value_parser = ["auto", "systemd", "openrc"])]
        service_init: String,

        #[command(subcommand)]
        service_command: ServiceCommands,
    },

    /// Run diagnostics for daemon/scheduler/channel freshness
    Doctor {
        #[command(subcommand)]
        doctor_command: Option<DoctorCommands>,
    },

    /// Show system status (full details)
    Status,

    /// Engage, inspect, and resume emergency-stop states.
    ///
    /// Examples:
    /// - `zeroclaw estop`
    /// - `zeroclaw estop --level network-kill`
    /// - `zeroclaw estop --level domain-block --domain "*.chase.com"`
    /// - `zeroclaw estop --level tool-freeze --tool shell --tool browser`
    /// - `zeroclaw estop status`
    /// - `zeroclaw estop resume --network`
    /// - `zeroclaw estop resume --domain "*.chase.com"`
    /// - `zeroclaw estop resume --tool shell`
    Estop {
        #[command(subcommand)]
        estop_command: Option<EstopSubcommands>,

        /// Level used when engaging estop from `zeroclaw estop`.
        #[arg(long, value_enum)]
        level: Option<EstopLevelArg>,

        /// Domain pattern(s) for `domain-block` (repeatable).
        #[arg(long = "domain")]
        domains: Vec<String>,

        /// Tool name(s) for `tool-freeze` (repeatable).
        #[arg(long = "tool")]
        tools: Vec<String>,
    },

    /// Configure and manage scheduled tasks
    #[command(long_about = "\
Configure and manage scheduled tasks.

Schedule recurring, one-shot, or interval-based tasks using cron \
expressions, RFC 3339 timestamps, durations, or fixed intervals.

Cron expressions use the standard 5-field format: \
'min hour day month weekday'. Timezones default to UTC; \
override with --tz and an IANA timezone name.

Examples:
  zeroclaw cron list
  zeroclaw cron add '0 9 * * 1-5' 'Good morning' --tz America/New_York
  zeroclaw cron add '*/30 * * * *' 'Check system health'
  zeroclaw cron add-at 2025-01-15T14:00:00Z 'Send reminder'
  zeroclaw cron add-every 60000 'Ping heartbeat'
  zeroclaw cron once 30m 'Run backup in 30 minutes'
  zeroclaw cron pause <task-id>
  zeroclaw cron update <task-id> --expression '0 8 * * *' --tz Europe/London")]
    Cron {
        #[command(subcommand)]
        cron_command: CronCommands,
    },

    /// Manage provider model catalogs
    Models {
        #[command(subcommand)]
        model_command: ModelCommands,
    },

    /// Manage preset compositions, import/export, and intent-driven planning
    Preset {
        #[command(subcommand)]
        preset_command: PresetCommands,
    },

    /// Inspect and change security/autonomy profile
    Security {
        #[command(subcommand)]
        security_command: SecurityCommands,
    },

    /// List supported AI providers
    Providers,

    /// Manage channels (telegram, discord, slack)
    #[command(long_about = "\
Manage communication channels.

Add, remove, list, and health-check channels that connect ZeroClaw \
to messaging platforms. Supported channel types: telegram, discord, \
slack, whatsapp, matrix, imessage, email.

Examples:
  zeroclaw channel list
  zeroclaw channel doctor
  zeroclaw channel add telegram '{\"bot_token\":\"...\",\"name\":\"my-bot\"}'
  zeroclaw channel remove my-bot
  zeroclaw channel bind-telegram zeroclaw_user")]
    Channel {
        #[command(subcommand)]
        channel_command: ChannelCommands,
    },

    /// Browse 50+ integrations
    Integrations {
        #[command(subcommand)]
        integration_command: IntegrationCommands,
    },

    /// Manage skills (user-defined capabilities)
    Skills {
        #[command(subcommand)]
        skill_command: SkillCommands,
    },

    /// Migrate data from other agent runtimes
    Migrate {
        #[command(subcommand)]
        migrate_command: MigrateCommands,
    },

    /// Manage provider subscription authentication profiles
    Auth {
        #[command(subcommand)]
        auth_command: AuthCommands,
    },

    /// Discover and introspect USB hardware
    #[command(long_about = "\
Discover and introspect USB hardware.

Enumerate connected USB devices, identify known development boards \
(STM32 Nucleo, Arduino, ESP32), and retrieve chip information via \
probe-rs / ST-Link.

Examples:
  zeroclaw hardware discover
  zeroclaw hardware introspect /dev/ttyACM0
  zeroclaw hardware info --chip STM32F401RETx")]
    Hardware {
        #[command(subcommand)]
        hardware_command: zeroclaw::HardwareCommands,
    },

    /// Manage hardware peripherals (STM32, RPi GPIO, etc.)
    #[command(long_about = "\
Manage hardware peripherals.

Add, list, flash, and configure hardware boards that expose tools \
to the agent (GPIO, sensors, actuators). Supported boards: \
nucleo-f401re, rpi-gpio, esp32, arduino-uno.

Examples:
  zeroclaw peripheral list
  zeroclaw peripheral add nucleo-f401re /dev/ttyACM0
  zeroclaw peripheral add rpi-gpio native
  zeroclaw peripheral flash --port /dev/cu.usbmodem12345
  zeroclaw peripheral flash-nucleo")]
    Peripheral {
        #[command(subcommand)]
        peripheral_command: zeroclaw::PeripheralCommands,
    },

    /// Manage agent memory (list, get, stats, clear)
    #[command(long_about = "\
Manage agent memory entries.

List, inspect, and clear memory entries stored by the agent. \
Supports filtering by category and session, pagination, and \
batch clearing with confirmation.

Examples:
  zeroclaw memory stats
  zeroclaw memory list
  zeroclaw memory list --category core --limit 10
  zeroclaw memory get <key>
  zeroclaw memory clear --category conversation --yes")]
    Memory {
        #[command(subcommand)]
        memory_command: MemoryCommands,
    },

    /// Manage configuration
    #[command(long_about = "\
Manage ZeroClaw configuration.

Inspect and export configuration settings. Use 'schema' to dump \
the full JSON Schema for the config file, which documents every \
available key, type, and default value.

Examples:
  zeroclaw config schema              # print JSON Schema to stdout
  zeroclaw config schema > schema.json")]
    Config {
        #[command(subcommand)]
        config_command: ConfigCommands,
    },

    /// Generate shell completion script to stdout
    #[command(long_about = "\
Generate shell completion scripts for `zeroclaw`.

The script is printed to stdout so it can be sourced directly:

Examples:
  source <(zeroclaw completions bash)
  zeroclaw completions zsh > ~/.zfunc/_zeroclaw
  zeroclaw completions fish > ~/.config/fish/completions/zeroclaw.fish")]
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Dump the full configuration JSON Schema to stdout
    Schema,
}

#[derive(Subcommand, Debug)]
enum EstopSubcommands {
    /// Print current estop status.
    Status,
    /// Resume from an engaged estop level.
    Resume {
        /// Resume only network kill.
        #[arg(long)]
        network: bool,
        /// Resume one or more blocked domain patterns.
        #[arg(long = "domain")]
        domains: Vec<String>,
        /// Resume one or more frozen tools.
        #[arg(long = "tool")]
        tools: Vec<String>,
        /// OTP code. If omitted and OTP is required, a prompt is shown.
        #[arg(long)]
        otp: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    /// Login with OAuth (OpenAI Codex or Gemini)
    Login {
        /// Provider (`openai-codex` or `gemini`)
        #[arg(long)]
        provider: String,
        /// Profile name (default: default)
        #[arg(long, default_value = "default")]
        profile: String,
        /// Use OAuth device-code flow
        #[arg(long)]
        device_code: bool,
    },
    /// Complete OAuth by pasting redirect URL or auth code
    PasteRedirect {
        /// Provider (`openai-codex`)
        #[arg(long)]
        provider: String,
        /// Profile name (default: default)
        #[arg(long, default_value = "default")]
        profile: String,
        /// Full redirect URL or raw OAuth code
        #[arg(long)]
        input: Option<String>,
    },
    /// Paste setup token / auth token (for Anthropic subscription auth)
    PasteToken {
        /// Provider (`anthropic`)
        #[arg(long)]
        provider: String,
        /// Profile name (default: default)
        #[arg(long, default_value = "default")]
        profile: String,
        /// Token value (if omitted, read interactively)
        #[arg(long)]
        token: Option<String>,
        /// Auth kind override (`authorization` or `api-key`)
        #[arg(long)]
        auth_kind: Option<String>,
    },
    /// Alias for `paste-token` (interactive by default)
    SetupToken {
        /// Provider (`anthropic`)
        #[arg(long)]
        provider: String,
        /// Profile name (default: default)
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Refresh OpenAI Codex access token using refresh token
    Refresh {
        /// Provider (`openai-codex`)
        #[arg(long)]
        provider: String,
        /// Profile name or profile id
        #[arg(long)]
        profile: Option<String>,
    },
    /// Remove auth profile
    Logout {
        /// Provider
        #[arg(long)]
        provider: String,
        /// Profile name (default: default)
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Set active profile for a provider
    Use {
        /// Provider
        #[arg(long)]
        provider: String,
        /// Profile name or full profile id
        #[arg(long)]
        profile: String,
    },
    /// List auth profiles
    List,
    /// Show auth status with active profile and token expiry info
    Status,
}

#[derive(Subcommand, Debug)]
enum ModelCommands {
    /// Refresh and cache provider models
    Refresh {
        /// Provider name (defaults to configured default provider)
        #[arg(long)]
        provider: Option<String>,

        /// Force live refresh and ignore fresh cache
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PresetCommands {
    /// List official presets and available packs
    List,
    /// Show details for an official preset
    Show {
        /// Official preset id
        id: String,
    },
    /// Show current workspace preset selection
    Current,
    /// Apply preset and pack changes to current workspace
    Apply {
        /// Base preset id (if omitted, starts from current selection or default)
        #[arg(long)]
        preset: Option<String>,

        /// Add a pack (repeatable)
        #[arg(long = "pack")]
        pack: Vec<String>,

        /// Remove a pack (repeatable)
        #[arg(long = "remove-pack")]
        remove_pack: Vec<String>,

        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Approve applying risky packs
        #[arg(long)]
        yes_risky: bool,

        /// Rebuild binary after applying selection
        #[arg(long)]
        rebuild: bool,

        /// Confirm rebuild execution
        #[arg(long)]
        yes_rebuild: bool,

        /// Emit machine-readable dry-run report (requires --dry-run)
        #[arg(long)]
        json: bool,
    },
    /// Build a preset plan from natural language intent
    Intent {
        /// Natural language intent text
        text: String,

        /// Extra capability graph file(s) to merge (repeatable)
        #[arg(long = "capabilities-file")]
        capabilities_file: Vec<std::path::PathBuf>,

        /// Apply the planned selection to workspace
        #[arg(long)]
        apply: bool,

        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Approve applying risky packs
        #[arg(long)]
        yes_risky: bool,

        /// Rebuild binary after applying selection
        #[arg(long)]
        rebuild: bool,

        /// Confirm rebuild execution
        #[arg(long)]
        yes_rebuild: bool,

        /// Print orchestration-friendly JSON report (plan + security recommendation + generated next commands)
        #[arg(long)]
        json: bool,

        /// Write a shell orchestration script (template only, not executed)
        #[arg(long = "emit-shell")]
        emit_shell: Option<std::path::PathBuf>,
    },
    /// Export preset payload JSON (share/import format)
    Export {
        /// Output file path
        path: std::path::PathBuf,

        /// Export an official preset id instead of current workspace selection
        #[arg(long)]
        preset: Option<String>,

        /// Emit machine-readable export report
        #[arg(long)]
        json: bool,
    },
    /// Import preset payload JSON into current workspace selection
    Import {
        /// Input file path
        path: std::path::PathBuf,

        /// Import mode: overwrite, merge, or fill
        #[arg(long, value_enum, default_value_t = presets::PresetImportMode::Merge)]
        mode: presets::PresetImportMode,

        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Approve applying risky packs
        #[arg(long)]
        yes_risky: bool,

        /// Rebuild binary after applying selection
        #[arg(long)]
        rebuild: bool,

        /// Confirm rebuild execution
        #[arg(long)]
        yes_rebuild: bool,

        /// Emit machine-readable dry-run report (requires --dry-run)
        #[arg(long)]
        json: bool,
    },
    /// Validate preset payload JSON files/directories
    Validate {
        /// Input file or directory path (repeatable)
        paths: Vec<std::path::PathBuf>,

        /// Allow unknown pack IDs (useful for external/private registries)
        #[arg(long)]
        allow_unknown_packs: bool,

        /// Print machine-readable JSON report
        #[arg(long)]
        json: bool,
    },
    /// Rebuild binary from current workspace preset selection
    Rebuild {
        /// Preview command only
        #[arg(long)]
        dry_run: bool,

        /// Confirm rebuild execution
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum SecurityProfileArg {
    Strict,
    Balanced,
    Flexible,
    Full,
}

impl SecurityProfileArg {
    fn as_profile_id(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Balanced => "balanced",
            Self::Flexible => "flexible",
            Self::Full => "full",
        }
    }

    fn is_non_strict(self) -> bool {
        !matches!(self, Self::Strict)
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum NonCliApprovalArg {
    Manual,
    Auto,
}

impl NonCliApprovalArg {
    fn allows_auto_approval(self) -> bool {
        matches!(self, Self::Auto)
    }
}

#[derive(Subcommand, Debug)]
enum SecurityCommands {
    /// Show current security profile and guardrails
    Show,
    /// Manage named security profiles
    Profile {
        #[command(subcommand)]
        profile_command: SecurityProfileCommands,
    },
}

#[derive(Subcommand, Debug)]
enum SecurityProfileCommands {
    /// Set workspace security profile
    Set {
        /// Target profile: strict, balanced, flexible, full
        #[arg(value_enum)]
        level: SecurityProfileArg,

        /// Non-CLI approval mode override: manual (default) or auto
        #[arg(long = "non-cli-approval", value_enum)]
        non_cli_approval: Option<NonCliApprovalArg>,

        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Confirm setting a non-strict profile
        #[arg(long = "yes-risk")]
        yes_risk: bool,

        /// Print structured JSON change report
        #[arg(long)]
        json: bool,

        /// Export change report to a JSON file
        #[arg(long = "export-diff")]
        export_diff: Option<std::path::PathBuf>,
    },
    /// Recommend a security profile from natural-language intent
    Recommend {
        /// Natural language intent text
        intent: String,

        /// Extra capability graph file(s) to merge (repeatable)
        #[arg(long = "capabilities-file")]
        capabilities_file: Vec<std::path::PathBuf>,

        /// Evaluate recommendation as if this preset were the base (does not write)
        #[arg(long = "from-preset")]
        from_preset: Option<String>,

        /// Add pack(s) on top of the planned selection (repeatable, does not write)
        #[arg(long = "pack")]
        pack: Vec<String>,

        /// Remove pack(s) from the planned selection (repeatable, does not write)
        #[arg(long = "remove-pack")]
        remove_pack: Vec<String>,

        /// Print structured JSON recommendation report
        #[arg(long)]
        json: bool,
    },
}

fn command_requests_machine_json(command: &Commands) -> bool {
    match command {
        Commands::Onboard { json, .. } => *json,
        Commands::Preset { preset_command } => match preset_command {
            PresetCommands::Apply { json, .. }
            | PresetCommands::Export { json, .. }
            | PresetCommands::Intent { json, .. }
            | PresetCommands::Import { json, .. }
            | PresetCommands::Validate { json, .. } => *json,
            _ => false,
        },
        Commands::Security { security_command } => match security_command {
            SecurityCommands::Profile { profile_command } => match profile_command {
                SecurityProfileCommands::Set { json, .. }
                | SecurityProfileCommands::Recommend { json, .. } => *json,
            },
            SecurityCommands::Show => false,
        },
        _ => false,
    }
}

#[derive(Subcommand, Debug)]
enum DoctorCommands {
    /// Probe model catalogs across providers and report availability
    Models {
        /// Probe a specific provider only (default: all known providers)
        #[arg(long)]
        provider: Option<String>,

        /// Prefer cached catalogs when available (skip forced live refresh)
        #[arg(long)]
        use_cache: bool,
    },
    /// Query runtime trace events (tool diagnostics and model replies)
    Traces {
        /// Show a specific trace event by id
        #[arg(long)]
        id: Option<String>,
        /// Filter list output by event type
        #[arg(long)]
        event: Option<String>,
        /// Case-insensitive text match across message/payload
        #[arg(long)]
        contains: Option<String>,
        /// Maximum number of events to display
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

#[derive(Subcommand, Debug)]
enum MemoryCommands {
    /// List memory entries with optional filters
    List {
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value = "50")]
        limit: usize,
        #[arg(long, default_value = "0")]
        offset: usize,
    },
    /// Get a specific memory entry by key
    Get { key: String },
    /// Show memory backend statistics and health
    Stats,
    /// Clear memories by category, by key, or clear all
    Clear {
        /// Delete a single entry by key (supports prefix match)
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        category: Option<String>,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

fn print_selection(selection: &presets::WorkspacePresetSelection) {
    println!("Preset: {}", selection.preset_id);
    println!("Packs:  {}", selection.packs.join(", "));
    if !selection.added_packs.is_empty() {
        println!("Added:  {}", selection.added_packs.join(", "));
    }
}

fn print_selection_diff(diff: &presets::SelectionDiff) {
    if let Some(before) = &diff.before_preset_id {
        if before != &diff.after_preset_id {
            println!("Preset: {before} -> {}", diff.after_preset_id);
        } else {
            println!("Preset: {}", diff.after_preset_id);
        }
    } else {
        println!("Preset: {}", diff.after_preset_id);
    }

    if diff.added_packs.is_empty() && diff.removed_packs.is_empty() {
        println!("Packs:  no changes");
        return;
    }

    if !diff.added_packs.is_empty() {
        println!("Add:    {}", diff.added_packs.join(", "));
    }
    if !diff.removed_packs.is_empty() {
        println!("Remove: {}", diff.removed_packs.join(", "));
    }
}

async fn maybe_rebuild_selection(
    selection: &presets::WorkspacePresetSelection,
    rebuild: bool,
    dry_run: bool,
    approved: bool,
) -> Result<()> {
    if !rebuild {
        return Ok(());
    }
    if !dry_run && !approved {
        bail!(
            "Refusing to run rebuild without confirmation. Re-run with `--yes-rebuild`, or use `--dry-run`."
        );
    }

    let cwd = std::env::current_dir()?;
    let plan = presets::rebuild_plan_for_selection(selection, &cwd)?;
    println!();
    println!("Rebuild command:");
    println!("  cargo {}", plan.args.join(" "));
    println!("  (working directory: {})", plan.manifest_dir.display());

    if dry_run {
        println!("Rebuild dry-run: no command executed.");
        return Ok(());
    }

    let plan_clone = plan.clone();
    tokio::task::spawn_blocking(move || presets::execute_rebuild_plan(&plan_clone))
        .await
        .map_err(|error| anyhow::anyhow!("rebuild task failed: {error}"))??;
    println!("Rebuild completed.");
    Ok(())
}

fn validate_onboard_command_mode(
    interactive: bool,
    channels_only: bool,
    force: bool,
    api_key_present: bool,
    provider_present: bool,
    model_present: bool,
    memory_present: bool,
    preset_present: bool,
    pack_present: bool,
    intent_present: bool,
    security_profile_present: bool,
    yes_security_risk: bool,
    dry_run: bool,
    json: bool,
    rebuild: bool,
    yes_rebuild: bool,
) -> Result<()> {
    if interactive && channels_only {
        bail!("Use either --interactive or --channels-only, not both");
    }
    if intent_present && (interactive || channels_only) {
        bail!(
            "`--intent` is supported in quick onboard mode only (without --interactive/--channels-only)."
        );
    }
    if intent_present && (preset_present || pack_present) {
        bail!("`--intent` cannot be combined with `--preset` or `--pack`.");
    }
    if dry_run && (interactive || channels_only) {
        bail!(
            "`--dry-run` is supported in quick onboard mode only (without --interactive/--channels-only)."
        );
    }
    if json && (interactive || channels_only) {
        bail!(
            "`--json` is supported in quick onboard mode only (without --interactive/--channels-only)."
        );
    }
    if json && !dry_run {
        bail!("`--json` requires `--dry-run` in onboard quick mode.");
    }
    if yes_rebuild && !rebuild {
        bail!("`--yes-rebuild` requires `--rebuild`.");
    }
    if channels_only
        && (api_key_present
            || provider_present
            || model_present
            || memory_present
            || preset_present
            || pack_present
            || intent_present
            || security_profile_present
            || yes_security_risk
            || dry_run
            || json
            || rebuild
            || yes_rebuild)
    {
        bail!(
            "--channels-only does not accept quick-setup or rebuild flags (--api-key/--provider/--model/--memory/--preset/--pack/--intent/--security-profile/--yes-security-risk/--dry-run/--json/--rebuild/--yes-rebuild)."
        );
    }
    if channels_only && force {
        bail!("--channels-only does not accept --force");
    }
    Ok(())
}

fn evaluate_onboard_quick_risk_requirements(
    risky_pack_ids: &[String],
    effective_security_profile: &str,
    yes_security_risk: bool,
    dry_run: bool,
) -> Result<OnboardQuickRiskAssessment> {
    let mut assessment = OnboardQuickRiskAssessment::default();

    if !risky_pack_ids.is_empty() && !yes_security_risk {
        if dry_run {
            assessment
                .consent_reasons
                .push(ConsentReasonCode::RiskyPack);
            assessment
                .warning_codes
                .push(OnboardWarningCode::RiskyPackRequiresConsent);
            assessment.warnings.push(format!(
                "Selection includes risky packs [{}]. Applying this plan requires `--yes-security-risk`.",
                risky_pack_ids.join(", ")
            ));
        } else {
            bail!(
                "Selection includes risky packs [{}]. Re-run with `--yes-security-risk`, or adjust preset/packs.",
                risky_pack_ids.join(", ")
            );
        }
    }

    if effective_security_profile != "strict" && !yes_security_risk {
        if dry_run {
            assessment
                .consent_reasons
                .push(ConsentReasonCode::SecurityNonStrict);
            assessment
                .warning_codes
                .push(OnboardWarningCode::SecurityNonStrictRequiresConsent);
            assessment.warnings.push(format!(
                "Security profile '{}' is non-strict. Applying this plan requires `--yes-security-risk`.",
                effective_security_profile
            ));
        } else {
            bail!(
                "Security profile '{}' requires explicit confirmation in quick mode. Re-run with `--yes-security-risk`.",
                effective_security_profile
            );
        }
    }

    Ok(assessment)
}

fn print_security_profile_summary(config: &Config) {
    let label = onboard::security_profile_label(&config.autonomy);
    println!("Security profile: {label}");
    println!(
        "Guardrails: workspace_only={}, medium_approval={}, high_risk_block={}, non_cli_approval={}",
        config.autonomy.workspace_only,
        config.autonomy.require_approval_for_medium_risk,
        config.autonomy.block_high_risk_commands,
        non_cli_approval_mode(config.autonomy.allow_non_cli_auto_approval)
    );
    println!(
        "Limits: max_actions_per_hour={}, max_cost_per_day=${:.2}",
        config.autonomy.max_actions_per_hour,
        config.autonomy.max_cost_per_day_cents as f32 / 100.0
    );
}

#[derive(Debug, Serialize)]
struct SecurityProfileSnapshot {
    profile_id: String,
    label: String,
    level: String,
    workspace_only: bool,
    require_approval_for_medium_risk: bool,
    block_high_risk_commands: bool,
    allow_non_cli_auto_approval: bool,
    non_cli_approval_mode: String,
    max_actions_per_hour: u32,
    max_cost_per_day_cents: u32,
    max_cost_per_day_usd: String,
}

#[derive(Debug, Serialize)]
struct SecurityFieldChange {
    field: String,
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
struct SecurityProfileChangeReport {
    schema_version: u32,
    report_type: String,
    current: SecurityProfileSnapshot,
    target: SecurityProfileSnapshot,
    changes: Vec<SecurityFieldChange>,
    requires_explicit_risk_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    risk_consent_reasons: Vec<SecurityRiskConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    risk_consent_reason_keys: Vec<String>,
    dry_run: bool,
    rollback_command: String,
}

#[derive(Debug, Serialize)]
struct SecurityProfileIntentRecommendationReport {
    schema_version: u32,
    report_type: String,
    intent: String,
    current_profile: SecurityProfileSnapshot,
    recommended_profile: onboard::SecurityProfileRecommendation,
    base_override_preset: Option<String>,
    manual_add_packs: Vec<String>,
    manual_remove_packs: Vec<String>,
    current_selection: Option<presets::WorkspacePresetSelection>,
    planned_selection: presets::WorkspacePresetSelection,
    risky_packs: Vec<String>,
    capability_sources: Vec<String>,
    plan_confidence: f32,
    plan_reasons: Vec<String>,
    apply_requires_explicit_risk_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reasons: Vec<ConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reason_keys: Vec<String>,
    apply_command: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ConsentReasonCode {
    RiskyPack,
    Rebuild,
    SecurityNonStrict,
}

impl ConsentReasonCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RiskyPack => "risky_pack",
            Self::Rebuild => "rebuild",
            Self::SecurityNonStrict => "security_non_strict",
        }
    }

    const fn i18n_key(self) -> &'static str {
        match self {
            Self::RiskyPack => "consent.reason.risky_pack",
            Self::Rebuild => "consent.reason.rebuild",
            Self::SecurityNonStrict => "consent.reason.security_non_strict",
        }
    }
}

impl std::fmt::Display for ConsentReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SecurityRiskConsentReasonCode {
    NonStrictProfile,
    NonCliAutoApproval,
}

impl SecurityRiskConsentReasonCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::NonStrictProfile => "non_strict_profile",
            Self::NonCliAutoApproval => "non_cli_auto_approval",
        }
    }

    const fn i18n_key(self) -> &'static str {
        match self {
            Self::NonStrictProfile => "security.risk_reason.non_strict_profile",
            Self::NonCliAutoApproval => "security.risk_reason.non_cli_auto_approval",
        }
    }
}

impl std::fmt::Display for SecurityRiskConsentReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OnboardWarningCode {
    RiskyPackRequiresConsent,
    SecurityNonStrictRequiresConsent,
}

impl OnboardWarningCode {
    const fn i18n_key(self) -> &'static str {
        match self {
            Self::RiskyPackRequiresConsent => "onboard.warning.risky_pack_requires_consent",
            Self::SecurityNonStrictRequiresConsent => {
                "onboard.warning.security_non_strict_requires_consent"
            }
        }
    }
}

const ONBOARD_QUICK_DRY_RUN_SCHEMA_VERSION: u32 = 1;
const ONBOARD_QUICK_DRY_RUN_REPORT_TYPE: &str = "onboard.quick_dry_run";
const PRESET_INTENT_ORCHESTRATION_SCHEMA_VERSION: u32 = 1;
const PRESET_INTENT_ORCHESTRATION_REPORT_TYPE: &str = "preset.intent_orchestration";
const PRESET_EXPORT_REPORT_SCHEMA_VERSION: u32 = 1;
const PRESET_EXPORT_REPORT_TYPE: &str = "preset.export";
const PRESET_APPLY_DRY_RUN_SCHEMA_VERSION: u32 = 1;
const PRESET_APPLY_DRY_RUN_REPORT_TYPE: &str = "preset.apply_dry_run";
const PRESET_IMPORT_DRY_RUN_SCHEMA_VERSION: u32 = 1;
const PRESET_IMPORT_DRY_RUN_REPORT_TYPE: &str = "preset.import_dry_run";
const SECURITY_PROFILE_CHANGE_SCHEMA_VERSION: u32 = 1;
const SECURITY_PROFILE_CHANGE_REPORT_TYPE: &str = "security.profile_change";
const SECURITY_PROFILE_RECOMMEND_SCHEMA_VERSION: u32 = 1;
const SECURITY_PROFILE_RECOMMEND_REPORT_TYPE: &str = "security.profile_recommendation";

#[derive(Debug, Serialize)]
struct OnboardIntentPlanPreview {
    intent: String,
    preset: String,
    add_packs: Vec<String>,
    remove_packs: Vec<String>,
    confidence: f32,
    risky_packs: Vec<String>,
    recommended_security_profile_id: String,
    recommended_security_profile_label: String,
}

#[derive(Debug, Serialize)]
struct OnboardRebuildPreview {
    command: String,
    working_directory: String,
    would_execute: bool,
}

#[derive(Debug, Serialize)]
struct PresetApplyDryRunReport {
    schema_version: u32,
    report_type: String,
    previous_selection: Option<presets::WorkspacePresetSelection>,
    planned_selection: presets::WorkspacePresetSelection,
    selection_diff: presets::SelectionDiff,
    risky_packs: Vec<String>,
    apply_requires_explicit_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reasons: Vec<ConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reason_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    rebuild_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rebuild_preview: Option<OnboardRebuildPreview>,
    workspace_written: bool,
}

#[derive(Debug, Serialize)]
struct PresetImportDryRunReport {
    schema_version: u32,
    report_type: String,
    import_mode: String,
    source_path: String,
    previous_selection: Option<presets::WorkspacePresetSelection>,
    planned_selection: presets::WorkspacePresetSelection,
    selection_diff: presets::SelectionDiff,
    risky_packs: Vec<String>,
    apply_requires_explicit_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reasons: Vec<ConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apply_consent_reason_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    rebuild_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rebuild_preview: Option<OnboardRebuildPreview>,
    workspace_written: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum PresetExportSourceKind {
    OfficialPreset,
    WorkspaceSelection,
    DefaultSelection,
}

#[derive(Debug, Serialize)]
struct PresetExportReport {
    schema_version: u32,
    report_type: String,
    source_kind: PresetExportSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_preset: Option<String>,
    selection: presets::WorkspacePresetSelection,
    target_path: String,
    bytes_written: usize,
    payload_sha256: String,
    write_performed: bool,
}

#[derive(Debug, Serialize)]
struct OnboardQuickDryRunReport {
    schema_version: u32,
    report_type: String,
    mode: String,
    intent_plan: Option<OnboardIntentPlanPreview>,
    planned_selection: presets::WorkspacePresetSelection,
    risky_packs: Vec<String>,
    security_profile: String,
    requires_explicit_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    consent_reasons: Vec<ConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    consent_reason_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warning_codes: Vec<OnboardWarningCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warning_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    rebuild_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rebuild_preview: Option<OnboardRebuildPreview>,
    config_written: bool,
    workspace_written: bool,
}

#[derive(Debug, Clone, Default)]
struct OnboardQuickRiskAssessment {
    consent_reasons: Vec<ConsentReasonCode>,
    warning_codes: Vec<OnboardWarningCode>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GeneratedNextCommand {
    id: String,
    description: String,
    command: String,
    requires_explicit_consent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    consent_reasons: Vec<ConsentReasonCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    consent_reason_keys: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PresetIntentOrchestrationReport {
    schema_version: u32,
    report_type: String,
    intent: String,
    capability_sources: Vec<String>,
    plan: presets::IntentPlan,
    planned_selection: presets::WorkspacePresetSelection,
    risky_packs: Vec<String>,
    security_recommendation: onboard::SecurityProfileRecommendation,
    security_apply_command: String,
    next_commands: Vec<GeneratedNextCommand>,
}

fn shell_quote(raw: &str) -> String {
    let escaped = raw.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn format_consent_reasons(reasons: &[ConsentReasonCode]) -> String {
    reasons
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn consent_reason_keys(reasons: &[ConsentReasonCode]) -> Vec<String> {
    reasons
        .iter()
        .map(|reason| reason.i18n_key().to_string())
        .collect()
}

fn onboard_warning_keys(codes: &[OnboardWarningCode]) -> Vec<String> {
    codes
        .iter()
        .map(|code| code.i18n_key().to_string())
        .collect()
}

fn format_security_risk_consent_reasons(reasons: &[SecurityRiskConsentReasonCode]) -> String {
    reasons
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn security_risk_consent_reason_keys(reasons: &[SecurityRiskConsentReasonCode]) -> Vec<String> {
    reasons
        .iter()
        .map(|reason| reason.i18n_key().to_string())
        .collect()
}

fn build_security_risk_consent_reasons(
    non_strict_profile: bool,
    enabling_non_cli_auto_approval: bool,
) -> Vec<SecurityRiskConsentReasonCode> {
    let mut reasons = Vec::new();
    if non_strict_profile {
        reasons.push(SecurityRiskConsentReasonCode::NonStrictProfile);
    }
    if enabling_non_cli_auto_approval {
        reasons.push(SecurityRiskConsentReasonCode::NonCliAutoApproval);
    }
    reasons
}

fn build_preset_intent_command(
    text: &str,
    capabilities_file: &[std::path::PathBuf],
    apply: bool,
    dry_run: bool,
    yes_risky: bool,
    rebuild: bool,
    yes_rebuild: bool,
) -> String {
    let mut parts = vec![
        "zeroclaw".to_string(),
        "preset".to_string(),
        "intent".to_string(),
    ];
    parts.push(shell_quote(text));
    for path in capabilities_file {
        parts.push("--capabilities-file".to_string());
        parts.push(shell_quote(&path.display().to_string()));
    }
    if apply {
        parts.push("--apply".to_string());
    }
    if dry_run {
        parts.push("--dry-run".to_string());
    }
    if yes_risky {
        parts.push("--yes-risky".to_string());
    }
    if rebuild {
        parts.push("--rebuild".to_string());
    }
    if yes_rebuild {
        parts.push("--yes-rebuild".to_string());
    }
    parts.join(" ")
}

fn build_security_apply_command(recommendation: &onboard::SecurityProfileRecommendation) -> String {
    if recommendation.requires_explicit_consent {
        format!(
            "zeroclaw security profile set {} --yes-risk",
            recommendation.profile_id
        )
    } else {
        format!(
            "zeroclaw security profile set {}",
            recommendation.profile_id
        )
    }
}

fn build_rebuild_preview(
    selection: &presets::WorkspacePresetSelection,
    rebuild: bool,
) -> Result<Option<OnboardRebuildPreview>> {
    if !rebuild {
        return Ok(None);
    }

    let cwd = std::env::current_dir()?;
    let plan = presets::rebuild_plan_for_selection(selection, &cwd)?;
    Ok(Some(OnboardRebuildPreview {
        command: format!("cargo {}", plan.args.join(" ")),
        working_directory: plan.manifest_dir.display().to_string(),
        would_execute: false,
    }))
}

fn build_preset_execution_consent_reasons(
    risky_packs: &[String],
    yes_risky: bool,
    rebuild: bool,
    yes_rebuild: bool,
) -> Vec<ConsentReasonCode> {
    let mut reasons = Vec::new();
    if !risky_packs.is_empty() && !yes_risky {
        reasons.push(ConsentReasonCode::RiskyPack);
    }
    if rebuild && !yes_rebuild {
        reasons.push(ConsentReasonCode::Rebuild);
    }
    reasons
}

fn build_preset_execution_warnings(
    risky_packs: &[String],
    rebuild: bool,
    execution_consent_reasons: &[ConsentReasonCode],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if execution_consent_reasons.contains(&ConsentReasonCode::RiskyPack) {
        warnings.push(format!(
            "Selection includes risky packs [{}]. Applying this plan requires `--yes-risky`.",
            risky_packs.join(", ")
        ));
    }
    if execution_consent_reasons.contains(&ConsentReasonCode::Rebuild) && rebuild {
        warnings.push(
            "Rebuild was requested. Executing this plan requires `--yes-rebuild`.".to_string(),
        );
    }
    warnings
}

fn build_preset_apply_consent_reasons(
    risky_packs: &[String],
    dry_run: bool,
    yes_risky: bool,
    rebuild: bool,
    yes_rebuild: bool,
) -> Vec<ConsentReasonCode> {
    if dry_run {
        Vec::new()
    } else {
        build_preset_execution_consent_reasons(risky_packs, yes_risky, rebuild, yes_rebuild)
    }
}

fn build_security_apply_consent_reasons(
    recommendation: &onboard::SecurityProfileRecommendation,
) -> Vec<ConsentReasonCode> {
    if recommendation.requires_explicit_consent {
        vec![ConsentReasonCode::SecurityNonStrict]
    } else {
        Vec::new()
    }
}

fn build_orchestration_shell_script(report: &PresetIntentOrchestrationReport) -> String {
    let mut lines = vec![
        "#!/usr/bin/env bash".to_string(),
        "set -euo pipefail".to_string(),
        "".to_string(),
        format!(
            "# Generated by: zeroclaw preset intent {} --json",
            shell_quote(&report.intent)
        ),
        "# This script is generated only. It is not executed automatically.".to_string(),
        "".to_string(),
        "confirm() {".to_string(),
        "  local prompt=\"$1\"".to_string(),
        "  local reply".to_string(),
        "  read -r -p \"$prompt [y/N]: \" reply".to_string(),
        "  case \"$reply\" in".to_string(),
        "    [yY]|[yY][eE][sS]) return 0 ;;".to_string(),
        "    *) return 1 ;;".to_string(),
        "  esac".to_string(),
        "}".to_string(),
        "".to_string(),
    ];

    for command in &report.next_commands {
        lines.push(format!("# {}: {}", command.id, command.description));
        if command.requires_explicit_consent {
            let reason_label = if command.consent_reasons.is_empty() {
                "manual_confirmation".to_string()
            } else {
                format_consent_reasons(&command.consent_reasons)
            };
            lines.push(format!(
                "if confirm \"Run {} (reasons: {})?\"; then",
                command.id, reason_label
            ));
            lines.push(format!("  {}", command.command));
            lines.push("else".to_string());
            lines.push(format!("  echo \"Skipped {}\"", command.id));
            lines.push("fi".to_string());
        } else {
            lines.push(command.command.clone());
        }
        lines.push("".to_string());
    }

    lines.join("\n")
}

fn emit_orchestration_shell_script(
    path: &std::path::Path,
    report: &PresetIntentOrchestrationReport,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
    }

    let script = build_orchestration_shell_script(report);
    std::fs::write(path, script).with_context(|| format!("Failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set executable bit on {}", path.display()))?;
    }

    Ok(())
}

fn autonomy_level_id(level: security::AutonomyLevel) -> &'static str {
    match level {
        security::AutonomyLevel::ReadOnly => "read_only",
        security::AutonomyLevel::Supervised => "supervised",
        security::AutonomyLevel::Full => "full",
    }
}

fn non_cli_approval_mode(auto_enabled: bool) -> &'static str {
    if auto_enabled {
        "auto"
    } else {
        "manual"
    }
}

fn cents_to_usd_string(cents: u32) -> String {
    format!("{:.2}", cents as f64 / 100.0)
}

fn build_security_profile_snapshot(
    autonomy: &config::AutonomyConfig,
    profile_id_override: Option<&str>,
) -> SecurityProfileSnapshot {
    let label = onboard::security_profile_label(autonomy).to_string();
    let profile_id = profile_id_override
        .map(str::to_string)
        .unwrap_or_else(|| onboard::security_profile_id_from_autonomy(autonomy).to_string());

    SecurityProfileSnapshot {
        profile_id,
        label,
        level: autonomy_level_id(autonomy.level).to_string(),
        workspace_only: autonomy.workspace_only,
        require_approval_for_medium_risk: autonomy.require_approval_for_medium_risk,
        block_high_risk_commands: autonomy.block_high_risk_commands,
        allow_non_cli_auto_approval: autonomy.allow_non_cli_auto_approval,
        non_cli_approval_mode: non_cli_approval_mode(autonomy.allow_non_cli_auto_approval)
            .to_string(),
        max_actions_per_hour: autonomy.max_actions_per_hour,
        max_cost_per_day_cents: autonomy.max_cost_per_day_cents,
        max_cost_per_day_usd: cents_to_usd_string(autonomy.max_cost_per_day_cents),
    }
}

fn build_security_profile_change_report(
    current: &config::AutonomyConfig,
    target: &config::AutonomyConfig,
    target_profile_id: &str,
    risk_consent_reasons: &[SecurityRiskConsentReasonCode],
    dry_run: bool,
) -> SecurityProfileChangeReport {
    let current_snapshot = build_security_profile_snapshot(current, None);
    let target_snapshot = build_security_profile_snapshot(target, Some(target_profile_id));
    let mut changes = Vec::new();

    if current_snapshot.profile_id != target_snapshot.profile_id {
        changes.push(SecurityFieldChange {
            field: "profile_id".to_string(),
            from: current_snapshot.profile_id.clone(),
            to: target_snapshot.profile_id.clone(),
        });
    }
    if current_snapshot.level != target_snapshot.level {
        changes.push(SecurityFieldChange {
            field: "level".to_string(),
            from: current_snapshot.level.clone(),
            to: target_snapshot.level.clone(),
        });
    }
    if current.workspace_only != target.workspace_only {
        changes.push(SecurityFieldChange {
            field: "workspace_only".to_string(),
            from: current.workspace_only.to_string(),
            to: target.workspace_only.to_string(),
        });
    }
    if current.require_approval_for_medium_risk != target.require_approval_for_medium_risk {
        changes.push(SecurityFieldChange {
            field: "require_approval_for_medium_risk".to_string(),
            from: current.require_approval_for_medium_risk.to_string(),
            to: target.require_approval_for_medium_risk.to_string(),
        });
    }
    if current.block_high_risk_commands != target.block_high_risk_commands {
        changes.push(SecurityFieldChange {
            field: "block_high_risk_commands".to_string(),
            from: current.block_high_risk_commands.to_string(),
            to: target.block_high_risk_commands.to_string(),
        });
    }
    if current.allow_non_cli_auto_approval != target.allow_non_cli_auto_approval {
        changes.push(SecurityFieldChange {
            field: "allow_non_cli_auto_approval".to_string(),
            from: current.allow_non_cli_auto_approval.to_string(),
            to: target.allow_non_cli_auto_approval.to_string(),
        });
        changes.push(SecurityFieldChange {
            field: "non_cli_approval_mode".to_string(),
            from: non_cli_approval_mode(current.allow_non_cli_auto_approval).to_string(),
            to: non_cli_approval_mode(target.allow_non_cli_auto_approval).to_string(),
        });
    }
    if current.max_actions_per_hour != target.max_actions_per_hour {
        changes.push(SecurityFieldChange {
            field: "max_actions_per_hour".to_string(),
            from: current.max_actions_per_hour.to_string(),
            to: target.max_actions_per_hour.to_string(),
        });
    }
    if current.max_cost_per_day_cents != target.max_cost_per_day_cents {
        changes.push(SecurityFieldChange {
            field: "max_cost_per_day_cents".to_string(),
            from: current.max_cost_per_day_cents.to_string(),
            to: target.max_cost_per_day_cents.to_string(),
        });
        changes.push(SecurityFieldChange {
            field: "max_cost_per_day_usd".to_string(),
            from: cents_to_usd_string(current.max_cost_per_day_cents),
            to: cents_to_usd_string(target.max_cost_per_day_cents),
        });
    }

    SecurityProfileChangeReport {
        schema_version: SECURITY_PROFILE_CHANGE_SCHEMA_VERSION,
        report_type: SECURITY_PROFILE_CHANGE_REPORT_TYPE.to_string(),
        current: current_snapshot,
        target: target_snapshot,
        changes,
        requires_explicit_risk_consent: !risk_consent_reasons.is_empty(),
        risk_consent_reasons: risk_consent_reasons.to_vec(),
        risk_consent_reason_keys: security_risk_consent_reason_keys(risk_consent_reasons),
        dry_run,
        rollback_command: "zeroclaw security profile set strict".to_string(),
    }
}

fn print_security_profile_change_report(report: &SecurityProfileChangeReport) {
    println!("Security profile change:");
    println!("- current: {}", report.current.label);
    println!(
        "  guardrails: workspace_only={}, medium_approval={}, high_risk_block={}, non_cli_approval={}, max_actions_per_hour={}, max_cost_per_day=${}",
        report.current.workspace_only,
        report.current.require_approval_for_medium_risk,
        report.current.block_high_risk_commands,
        report.current.non_cli_approval_mode,
        report.current.max_actions_per_hour,
        report.current.max_cost_per_day_usd
    );
    println!("- target: {}", report.target.label);
    println!(
        "  guardrails: workspace_only={}, medium_approval={}, high_risk_block={}, non_cli_approval={}, max_actions_per_hour={}, max_cost_per_day=${}",
        report.target.workspace_only,
        report.target.require_approval_for_medium_risk,
        report.target.block_high_risk_commands,
        report.target.non_cli_approval_mode,
        report.target.max_actions_per_hour,
        report.target.max_cost_per_day_usd
    );

    if report.changes.is_empty() {
        println!("- delta: no effective policy changes");
    } else {
        println!("- delta:");
        for change in &report.changes {
            println!("  {}: {} -> {}", change.field, change.from, change.to);
        }
    }
    if report.requires_explicit_risk_consent {
        println!(
            "- explicit risk consent: yes ({})",
            format_security_risk_consent_reasons(&report.risk_consent_reasons)
        );
    } else {
        println!("- explicit risk consent: no");
    }
}

async fn handle_security_command(command: SecurityCommands, config: &mut Config) -> Result<()> {
    match command {
        SecurityCommands::Show => {
            print_security_profile_summary(config);
            Ok(())
        }
        SecurityCommands::Profile { profile_command } => match profile_command {
            SecurityProfileCommands::Set {
                level,
                non_cli_approval,
                dry_run,
                yes_risk,
                json,
                export_diff,
            } => {
                let profile_id = level.as_profile_id();
                let current = config.autonomy.clone();
                let mut next = onboard::autonomy_config_for_security_profile_id(profile_id)?;
                if let Some(mode) = non_cli_approval {
                    next.allow_non_cli_auto_approval = mode.allows_auto_approval();
                }

                let enabling_non_cli_auto_approval =
                    !current.allow_non_cli_auto_approval && next.allow_non_cli_auto_approval;
                let risk_consent_reasons = build_security_risk_consent_reasons(
                    level.is_non_strict(),
                    enabling_non_cli_auto_approval,
                );
                let requires_explicit_risk_consent = !risk_consent_reasons.is_empty();
                let report = build_security_profile_change_report(
                    &current,
                    &next,
                    profile_id,
                    &risk_consent_reasons,
                    dry_run,
                );

                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_security_profile_change_report(&report);
                }

                if let Some(path) = export_diff {
                    let payload = serde_json::to_string_pretty(&report)?;
                    std::fs::write(&path, payload)
                        .with_context(|| format!("Failed to write {}", path.display()))?;
                    if json {
                        eprintln!("Exported security diff: {}", path.display());
                    } else {
                        println!("Exported security diff: {}", path.display());
                    }
                }

                if requires_explicit_risk_consent && !yes_risk && !dry_run {
                    let mut risk_reasons = Vec::new();
                    if level.is_non_strict() {
                        risk_reasons.push(format!("profile '{}' is non-strict", profile_id));
                    }
                    if enabling_non_cli_auto_approval {
                        risk_reasons.push(
                            "non-CLI auto approval removes per-call confirmation on non-CLI channels"
                                .to_string(),
                        );
                    }
                    bail!(
                        "Refusing to apply risk-elevating security changes without explicit consent ({}). Re-run with `--yes-risk`, or use `--dry-run`.",
                        risk_reasons.join("; ")
                    );
                }

                if dry_run {
                    if !json {
                        println!("Security profile dry-run: no changes written.");
                        println!("Rollback command: {}", report.rollback_command);
                    }
                    return Ok(());
                }

                config.autonomy = next;
                config.save().await?;
                if !json {
                    println!("Saved config: {}", config.config_path.display());
                    println!("Rollback command: {}", report.rollback_command);
                }
                Ok(())
            }
            SecurityProfileCommands::Recommend {
                intent,
                capabilities_file,
                from_preset,
                pack,
                remove_pack,
                json,
            } => {
                let current_selection = presets::load_workspace_selection(config)?;
                let resolved_capabilities =
                    presets::resolve_intent_capabilities(config, &capabilities_file)?;
                let plan = presets::plan_from_intent_with_rules(
                    &intent,
                    current_selection.as_ref(),
                    &resolved_capabilities.rules,
                );
                let planned_selection = if let Some(base_preset_id) = from_preset.as_deref() {
                    let base = presets::from_preset_id(base_preset_id)?;
                    presets::compose_selection(base, &plan.add_packs, &plan.remove_packs)?
                } else {
                    presets::selection_from_plan(&plan, current_selection.as_ref())?
                };
                let planned_selection =
                    presets::compose_selection(planned_selection, &pack, &remove_pack)?;
                let risky_packs = presets::risky_pack_ids(&planned_selection);
                let recommendation =
                    onboard::recommend_security_profile(Some(&intent), &planned_selection.packs);
                let apply_consent_reasons = build_security_apply_consent_reasons(&recommendation);
                let apply_command = build_security_apply_command(&recommendation);

                let report = SecurityProfileIntentRecommendationReport {
                    schema_version: SECURITY_PROFILE_RECOMMEND_SCHEMA_VERSION,
                    report_type: SECURITY_PROFILE_RECOMMEND_REPORT_TYPE.to_string(),
                    intent: intent.clone(),
                    current_profile: build_security_profile_snapshot(&config.autonomy, None),
                    recommended_profile: recommendation,
                    base_override_preset: from_preset.clone(),
                    manual_add_packs: pack.clone(),
                    manual_remove_packs: remove_pack.clone(),
                    current_selection,
                    planned_selection,
                    risky_packs,
                    capability_sources: resolved_capabilities.sources,
                    plan_confidence: plan.confidence,
                    plan_reasons: plan.reasons,
                    apply_requires_explicit_risk_consent: !apply_consent_reasons.is_empty(),
                    apply_consent_reason_keys: consent_reason_keys(&apply_consent_reasons),
                    apply_consent_reasons,
                    apply_command,
                };

                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    return Ok(());
                }

                println!("Intent: {}", report.intent);
                println!(
                    "Current profile: {} ({})",
                    report.current_profile.label, report.current_profile.profile_id
                );
                if let Some(base) = report.base_override_preset.as_deref() {
                    println!("Planning base override: {base}");
                }
                println!(
                    "Recommended profile: {} ({})",
                    report.recommended_profile.label, report.recommended_profile.profile_id
                );
                println!("Risk tier: {}", report.recommended_profile.risk_tier);
                println!("Recommendation reasons:");
                for reason in &report.recommended_profile.reasons {
                    println!("- {reason}");
                }
                println!();
                println!("Preset/pack plan used for recommendation:");
                println!("- preset: {}", report.planned_selection.preset_id);
                println!("- packs: {}", report.planned_selection.packs.join(", "));
                if !report.manual_add_packs.is_empty() {
                    println!("- manual add packs: {}", report.manual_add_packs.join(", "));
                }
                if !report.manual_remove_packs.is_empty() {
                    println!(
                        "- manual remove packs: {}",
                        report.manual_remove_packs.join(", ")
                    );
                }
                if report.risky_packs.is_empty() {
                    println!("- risky packs: (none)");
                } else {
                    println!("- risky packs: {}", report.risky_packs.join(", "));
                }
                println!("- plan confidence: {:.2}", report.plan_confidence);
                if !report.capability_sources.is_empty() {
                    println!(
                        "- capability sources: {}",
                        report.capability_sources.join(" -> ")
                    );
                }
                println!(
                    "- apply requires explicit risk consent: {}",
                    if report.apply_requires_explicit_risk_consent {
                        "yes"
                    } else {
                        "no"
                    }
                );
                if !report.apply_consent_reasons.is_empty() {
                    println!(
                        "- apply consent reasons: {}",
                        format_consent_reasons(&report.apply_consent_reasons)
                    );
                }
                println!();
                println!("No config changes were applied.");
                println!("Apply recommendation:");
                println!("  {}", report.apply_command);
                Ok(())
            }
        },
    }
}

async fn handle_preset_command(command: PresetCommands, config: &Config) -> Result<()> {
    match command {
        PresetCommands::List => {
            println!("Official presets:");
            for preset in onboard::PRESETS {
                println!("- {}: {}", preset.id, preset.description);
                println!("  packs: {}", preset.packs.join(", "));
            }
            println!();
            println!("Available packs:");
            for pack in onboard::FEATURE_PACKS {
                let risk = if pack.requires_confirmation {
                    " [requires confirmation]"
                } else {
                    ""
                };
                let features = if pack.cargo_features.is_empty() {
                    "(no extra cargo features)".to_string()
                } else {
                    pack.cargo_features.join(", ")
                };
                println!("- {}{}: {}", pack.id, risk, pack.description);
                println!("  cargo features: {features}");
            }
            Ok(())
        }
        PresetCommands::Show { id } => {
            let preset =
                onboard::preset_by_id(&id).with_context(|| format!("Unknown preset id '{id}'"))?;
            println!("Preset: {}", preset.id);
            println!("Description: {}", preset.description);
            println!("Packs:");
            for pack_id in preset.packs {
                if let Some(pack) = onboard::feature_pack_by_id(pack_id) {
                    let risk = if pack.requires_confirmation {
                        " [requires confirmation]"
                    } else {
                        ""
                    };
                    println!("- {}{}: {}", pack.id, risk, pack.description);
                } else {
                    println!("- {} (unknown pack reference)", pack_id);
                }
            }
            Ok(())
        }
        PresetCommands::Current => {
            let path = presets::workspace_preset_path(config);
            let current = presets::load_workspace_selection(config)?;
            println!("Workspace preset file: {}", path.display());
            if let Some(selection) = current {
                print_selection(&selection);
            } else {
                println!("No workspace preset selection found yet.");
            }
            Ok(())
        }
        PresetCommands::Apply {
            preset,
            pack,
            remove_pack,
            dry_run,
            yes_risky,
            rebuild,
            yes_rebuild,
            json,
        } => {
            if json && !dry_run {
                bail!("`preset apply --json` requires `--dry-run`.");
            }

            let before = presets::load_workspace_selection(config)?;
            let base = if let Some(preset_id) = preset {
                presets::from_preset_id(&preset_id)?
            } else if let Some(current) = before.clone() {
                current
            } else {
                presets::default_selection()?
            };
            let after = presets::compose_selection(base, &pack, &remove_pack)?;
            let diff = presets::selection_diff(before.as_ref(), &after);

            let risky = presets::risky_pack_ids(&after);
            let execution_consent_reasons =
                build_preset_execution_consent_reasons(&risky, yes_risky, rebuild, yes_rebuild);
            let execution_warnings =
                build_preset_execution_warnings(&risky, rebuild, &execution_consent_reasons);

            if json {
                let report = PresetApplyDryRunReport {
                    schema_version: PRESET_APPLY_DRY_RUN_SCHEMA_VERSION,
                    report_type: PRESET_APPLY_DRY_RUN_REPORT_TYPE.to_string(),
                    previous_selection: before.clone(),
                    planned_selection: after.clone(),
                    selection_diff: diff,
                    risky_packs: risky.clone(),
                    apply_requires_explicit_consent: !execution_consent_reasons.is_empty(),
                    apply_consent_reason_keys: consent_reason_keys(&execution_consent_reasons),
                    apply_consent_reasons: execution_consent_reasons,
                    warnings: execution_warnings,
                    rebuild_requested: rebuild,
                    rebuild_preview: build_rebuild_preview(&after, rebuild)?,
                    workspace_written: false,
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            println!("Preset plan:");
            print_selection_diff(&diff);

            if !risky.is_empty() && !yes_risky && !dry_run {
                bail!(
                    "Selection includes risky packs [{}]. Re-run with `--yes-risky`, or use `--dry-run`.",
                    risky.join(", ")
                );
            }
            if !risky.is_empty() {
                println!("Risky packs: {}", risky.join(", "));
            }

            if dry_run {
                println!("Apply dry-run: no changes written.");
                maybe_rebuild_selection(&after, rebuild, true, true).await?;
                return Ok(());
            }

            let path = presets::save_workspace_selection(config, &after)?;
            println!("Saved workspace preset selection: {}", path.display());
            maybe_rebuild_selection(&after, rebuild, false, yes_rebuild).await?;
            Ok(())
        }
        PresetCommands::Intent {
            text,
            capabilities_file,
            apply,
            dry_run,
            yes_risky,
            rebuild,
            yes_rebuild,
            json,
            emit_shell,
        } => {
            if json && apply {
                bail!("`preset intent --json` is plan-only and cannot be combined with `--apply`.");
            }
            if emit_shell.is_some() && apply {
                bail!("`preset intent --emit-shell` is plan-only and cannot be combined with `--apply`.");
            }

            let before = presets::load_workspace_selection(config)?;
            let resolved_capabilities =
                presets::resolve_intent_capabilities(config, &capabilities_file)?;
            let plan = presets::plan_from_intent_with_rules(
                &text,
                before.as_ref(),
                &resolved_capabilities.rules,
            );
            let after = presets::selection_from_plan(&plan, before.as_ref())?;
            let diff = presets::selection_diff(before.as_ref(), &after);
            let risky = presets::risky_pack_ids(&after);
            let security_recommendation =
                onboard::recommend_security_profile(Some(&text), &after.packs);
            let security_apply_command = build_security_apply_command(&security_recommendation);

            let preview_apply_command = build_preset_intent_command(
                &text,
                &capabilities_file,
                true,
                true,
                false,
                rebuild,
                false,
            );
            let apply_command = build_preset_intent_command(
                &text,
                &capabilities_file,
                true,
                dry_run,
                yes_risky,
                rebuild,
                yes_rebuild,
            );
            let preset_apply_consent_reasons = build_preset_apply_consent_reasons(
                &risky,
                dry_run,
                yes_risky,
                rebuild,
                yes_rebuild,
            );
            let security_apply_consent_reasons =
                build_security_apply_consent_reasons(&security_recommendation);

            let mut next_commands = vec![
                GeneratedNextCommand {
                    id: "preset.apply.preview".to_string(),
                    description:
                        "Preview applying this intent plan without mutating workspace state"
                            .to_string(),
                    command: preview_apply_command.clone(),
                    requires_explicit_consent: false,
                    consent_reasons: Vec::new(),
                    consent_reason_keys: Vec::new(),
                },
                GeneratedNextCommand {
                    id: "preset.apply".to_string(),
                    description: "Apply this preset composition plan to workspace selection"
                        .to_string(),
                    command: apply_command,
                    requires_explicit_consent: !preset_apply_consent_reasons.is_empty(),
                    consent_reason_keys: consent_reason_keys(&preset_apply_consent_reasons),
                    consent_reasons: preset_apply_consent_reasons,
                },
                GeneratedNextCommand {
                    id: "security.profile.set".to_string(),
                    description:
                        "Align security profile with the recommended guardrails (manual step)"
                            .to_string(),
                    command: security_apply_command.clone(),
                    requires_explicit_consent: !security_apply_consent_reasons.is_empty(),
                    consent_reason_keys: consent_reason_keys(&security_apply_consent_reasons),
                    consent_reasons: security_apply_consent_reasons,
                },
            ];
            if next_commands[0].command == next_commands[1].command {
                next_commands.remove(0);
            }

            let orchestration_report = PresetIntentOrchestrationReport {
                schema_version: PRESET_INTENT_ORCHESTRATION_SCHEMA_VERSION,
                report_type: PRESET_INTENT_ORCHESTRATION_REPORT_TYPE.to_string(),
                intent: text.clone(),
                capability_sources: resolved_capabilities.sources.clone(),
                plan: plan.clone(),
                planned_selection: after.clone(),
                risky_packs: risky.clone(),
                security_recommendation: security_recommendation.clone(),
                security_apply_command: security_apply_command.clone(),
                next_commands: next_commands.clone(),
            };

            if let Some(path) = emit_shell.as_ref() {
                emit_orchestration_shell_script(path, &orchestration_report)?;
                if json {
                    eprintln!("Wrote orchestration shell script: {}", path.display());
                } else {
                    println!("Wrote orchestration shell script: {}", path.display());
                }
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&orchestration_report)?);
                return Ok(());
            }

            println!("Intent: {}", plan.intent);
            if let Some(base) = plan.base_preset_id.as_deref() {
                println!("Base preset: {base}");
            } else {
                println!("Base preset: current selection/default fallback");
            }
            println!("Confidence: {:.2}", plan.confidence);
            if !resolved_capabilities.sources.is_empty() {
                println!(
                    "Capability sources: {}",
                    resolved_capabilities.sources.join(" -> ")
                );
            }
            println!("Reasons:");
            for reason in &plan.reasons {
                println!("- {reason}");
            }
            if !plan.capability_signals.is_empty() {
                println!();
                println!("Capability graph matches:");
                for signal in &plan.capability_signals {
                    println!(
                        "- {} ({:.2}) — {}",
                        signal.capability_id, signal.weight, signal.rationale
                    );
                    println!("  terms: {}", signal.matched_terms.join(", "));
                }
            }
            if !plan.preset_ranking.is_empty() {
                println!();
                println!("Preset ranking:");
                for rank in plan.preset_ranking.iter().take(3) {
                    println!("- {} ({:.2})", rank.preset_id, rank.score);
                    if !rank.reasons.is_empty() {
                        println!("  signals: {}", rank.reasons.join("; "));
                    }
                }
            }
            println!();
            println!(
                "Confidence breakdown: base {:.2} + signal {:.2} + ranking {:.2} - penalty {:.2} = {:.2}",
                plan.confidence_breakdown.base,
                plan.confidence_breakdown.signal_bonus,
                plan.confidence_breakdown.ranking_bonus,
                plan.confidence_breakdown.contradiction_penalty,
                plan.confidence_breakdown.final_score
            );
            println!();
            println!("Intent directives:");
            if plan.add_packs.is_empty() {
                println!("- add packs: (none)");
            } else {
                println!("- add packs: {}", plan.add_packs.join(", "));
            }
            if plan.remove_packs.is_empty() {
                println!("- remove packs: (none)");
            } else {
                println!("- remove packs: {}", plan.remove_packs.join(", "));
            }
            println!();
            println!("Planned selection:");
            print_selection_diff(&diff);
            println!("Resolved packs: {}", after.packs.join(", "));
            if before.is_none() {
                println!("Current workspace selection: none (first composition run)");
            }
            println!();
            println!(
                "Security recommendation: {} ({})",
                security_recommendation.label, security_recommendation.profile_id
            );
            println!("Risk tier: {}", security_recommendation.risk_tier);
            if let Some(primary_reason) = security_recommendation.reasons.first() {
                println!("Why: {primary_reason}");
            }

            if !apply {
                println!();
                println!("Generated next commands (not executed):");
                for entry in &orchestration_report.next_commands {
                    println!("- {}: {}", entry.id, entry.description);
                    println!(
                        "  consent required: {}",
                        if entry.requires_explicit_consent {
                            "yes"
                        } else {
                            "no"
                        }
                    );
                    if !entry.consent_reasons.is_empty() {
                        println!(
                            "  consent reasons: {}",
                            format_consent_reasons(&entry.consent_reasons)
                        );
                    }
                    println!("  {}", entry.command);
                }
                println!();
                println!("Plan only. Re-run with `--apply` to persist this selection.");
                return Ok(());
            }

            if !risky.is_empty() && !yes_risky && !dry_run {
                bail!(
                    "Selection includes risky packs [{}]. Re-run with `--yes-risky`, or use `--dry-run`.",
                    risky.join(", ")
                );
            }
            if !risky.is_empty() {
                println!("Risky packs: {}", risky.join(", "));
            }

            if dry_run {
                println!("Intent apply dry-run: no changes written.");
                maybe_rebuild_selection(&after, rebuild, true, true).await?;
                return Ok(());
            }

            let path = presets::save_workspace_selection(config, &after)?;
            println!("Saved workspace preset selection: {}", path.display());
            maybe_rebuild_selection(&after, rebuild, false, yes_rebuild).await?;
            println!("Recommended follow-up security command:");
            println!("  {security_apply_command}");
            Ok(())
        }
        PresetCommands::Export { path, preset, json } => {
            let (selection, source_kind, requested_preset) = if let Some(preset_id) = preset {
                (
                    presets::from_preset_id(&preset_id)?,
                    PresetExportSourceKind::OfficialPreset,
                    Some(preset_id),
                )
            } else if let Some(current) = presets::load_workspace_selection(config)? {
                (
                    current,
                    PresetExportSourceKind::WorkspaceSelection,
                    None::<String>,
                )
            } else {
                (
                    presets::default_selection()?,
                    PresetExportSourceKind::DefaultSelection,
                    None::<String>,
                )
            };
            let document = presets::selection_to_document(&selection);
            presets::export_document_to_path(&path, &document)?;

            if json {
                let payload = std::fs::read(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let payload_sha256 = format!("{:x}", Sha256::digest(&payload));
                let report = PresetExportReport {
                    schema_version: PRESET_EXPORT_REPORT_SCHEMA_VERSION,
                    report_type: PRESET_EXPORT_REPORT_TYPE.to_string(),
                    source_kind,
                    requested_preset,
                    selection,
                    target_path: path.display().to_string(),
                    bytes_written: payload.len(),
                    payload_sha256,
                    write_performed: true,
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            println!("Exported preset payload to {}", path.display());
            Ok(())
        }
        PresetCommands::Import {
            path,
            mode,
            dry_run,
            yes_risky,
            rebuild,
            yes_rebuild,
            json,
        } => {
            if json && !dry_run {
                bail!("`preset import --json` requires `--dry-run`.");
            }

            let result = presets::import_selection_from_path(config, &path, mode)?;
            let diff = presets::selection_diff(result.before.as_ref(), &result.after);
            let risky = presets::risky_pack_ids(&result.after);
            let execution_consent_reasons =
                build_preset_execution_consent_reasons(&risky, yes_risky, rebuild, yes_rebuild);
            let execution_warnings =
                build_preset_execution_warnings(&risky, rebuild, &execution_consent_reasons);

            if json {
                let report = PresetImportDryRunReport {
                    schema_version: PRESET_IMPORT_DRY_RUN_SCHEMA_VERSION,
                    report_type: PRESET_IMPORT_DRY_RUN_REPORT_TYPE.to_string(),
                    import_mode: result.mode.to_string(),
                    source_path: path.display().to_string(),
                    previous_selection: result.before.clone(),
                    planned_selection: result.after.clone(),
                    selection_diff: diff,
                    risky_packs: risky,
                    apply_requires_explicit_consent: !execution_consent_reasons.is_empty(),
                    apply_consent_reason_keys: consent_reason_keys(&execution_consent_reasons),
                    apply_consent_reasons: execution_consent_reasons,
                    warnings: execution_warnings,
                    rebuild_requested: rebuild,
                    rebuild_preview: build_rebuild_preview(&result.after, rebuild)?,
                    workspace_written: false,
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            println!("Import mode: {}", result.mode);
            print_selection_diff(&diff);

            if !risky.is_empty() && !yes_risky && !dry_run {
                bail!(
                    "Selection includes risky packs [{}]. Re-run with `--yes-risky`, or use `--dry-run`.",
                    risky.join(", ")
                );
            }
            if !risky.is_empty() {
                println!("Risky packs: {}", risky.join(", "));
            }

            if dry_run {
                println!("Import dry-run: no changes written.");
                maybe_rebuild_selection(&result.after, rebuild, true, true).await?;
                return Ok(());
            }

            let saved = presets::save_workspace_selection(config, &result.after)?;
            println!("Saved workspace preset selection: {}", saved.display());
            maybe_rebuild_selection(&result.after, rebuild, false, yes_rebuild).await?;
            Ok(())
        }
        PresetCommands::Validate {
            paths,
            allow_unknown_packs,
            json,
        } => {
            let report = presets::validate_preset_paths(&paths, allow_unknown_packs)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Preset validation summary: {} checked, {} failed",
                    report.files_checked, report.files_failed
                );
                println!(
                    "Unknown packs allowed: {}",
                    if report.allow_unknown_packs {
                        "yes"
                    } else {
                        "no"
                    }
                );
                for result in &report.results {
                    if result.ok {
                        println!("- [ok] {} ({})", result.path, result.format);
                    } else {
                        println!("- [failed] {} ({})", result.path, result.format);
                        for error in &result.errors {
                            println!("  - {error}");
                        }
                    }
                }
            }

            if report.files_failed > 0 {
                bail!(
                    "Preset validation failed for {} of {} files.",
                    report.files_failed,
                    report.files_checked
                );
            }
            Ok(())
        }
        PresetCommands::Rebuild { dry_run, yes } => {
            let selection = if let Some(current) = presets::load_workspace_selection(config)? {
                current
            } else {
                presets::default_selection()?
            };
            maybe_rebuild_selection(&selection, true, dry_run, yes).await
        }
    }
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<()> {
    // Install default crypto provider for Rustls TLS.
    // This prevents the error: "could not automatically determine the process-level CryptoProvider"
    // when both aws-lc-rs and ring features are available (or neither is explicitly selected).
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        eprintln!("Warning: Failed to install default crypto provider: {e:?}");
    }

    let cli = Cli::parse();

    if let Some(config_dir) = &cli.config_dir {
        if config_dir.trim().is_empty() {
            bail!("--config-dir cannot be empty");
        }
        std::env::set_var("ZEROCLAW_CONFIG_DIR", config_dir);
    }

    // Completions must remain stdout-only and should not load config or initialize logging.
    // This avoids warnings/log lines corrupting sourced completion scripts.
    if let Commands::Completions { shell } = &cli.command {
        let mut stdout = std::io::stdout().lock();
        write_shell_completion(*shell, &mut stdout)?;
        return Ok(());
    }

    let machine_json_mode = command_requests_machine_json(&cli.command);

    if !machine_json_mode {
        // Initialize logging - respects RUST_LOG env var, defaults to INFO.
        // Logs stay on stderr to keep stdout user/machine payloads clean.
        let subscriber = fmt::Subscriber::builder()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_writer(std::io::stderr)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    }

    // Onboard runs quick setup by default, or the interactive wizard with --interactive.
    // The onboard wizard uses reqwest::blocking internally, which creates its own
    // Tokio runtime. To avoid "Cannot drop a runtime in a context where blocking is
    // not allowed", we run the wizard on a blocking thread via spawn_blocking.
    if let Commands::Onboard {
        interactive,
        force,
        channels_only,
        api_key,
        provider,
        model,
        memory,
        preset,
        pack,
        intent,
        security_profile,
        yes_security_risk,
        dry_run,
        json,
        rebuild,
        yes_rebuild,
    } = &cli.command
    {
        let interactive = *interactive;
        let force = *force;
        let channels_only = *channels_only;
        let api_key = api_key.clone();
        let provider = provider.clone();
        let model = model.clone();
        let memory = memory.clone();
        let preset = preset.clone();
        let pack = pack.clone();
        let intent = intent.clone();
        let security_profile = *security_profile;
        let yes_security_risk = *yes_security_risk;
        let dry_run = *dry_run;
        let json = *json;
        let rebuild = *rebuild;
        let yes_rebuild = *yes_rebuild;

        validate_onboard_command_mode(
            interactive,
            channels_only,
            force,
            api_key.is_some(),
            provider.is_some(),
            model.is_some(),
            memory.is_some(),
            preset.is_some(),
            !pack.is_empty(),
            intent.is_some(),
            security_profile.is_some(),
            yes_security_risk,
            dry_run,
            json,
            rebuild,
            yes_rebuild,
        )?;
        let config = if channels_only {
            onboard::run_channels_repair_wizard().await
        } else if interactive {
            onboard::run_wizard(force).await
        } else {
            let mut resolved_preset = preset.clone();
            let mut resolved_pack = pack.clone();
            let mut resolved_remove_pack: Vec<String> = Vec::new();
            let mut intent_preview: Option<OnboardIntentPlanPreview> = None;
            let mut resolved_security_profile =
                security_profile.map(|value| value.as_profile_id().to_string());

            if let Some(intent_text) = intent.clone() {
                let minimal_base = presets::from_preset_id("minimal")?;
                let plan = presets::plan_from_intent(&intent_text, Some(&minimal_base));
                let planned_selection = presets::selection_from_plan(&plan, Some(&minimal_base))?;
                let planned_preset = onboard::preset_by_id(&planned_selection.preset_id)
                    .with_context(|| {
                        format!(
                            "Intent planner produced unknown preset '{}'",
                            planned_selection.preset_id
                        )
                    })?;

                resolved_preset = Some(planned_selection.preset_id.clone());
                resolved_pack = planned_selection
                    .packs
                    .iter()
                    .filter(|pack_id| {
                        !planned_preset
                            .packs
                            .iter()
                            .any(|base_pack| *base_pack == pack_id.as_str())
                    })
                    .cloned()
                    .collect();
                resolved_remove_pack = planned_preset
                    .packs
                    .iter()
                    .filter(|base_pack| {
                        !planned_selection
                            .packs
                            .iter()
                            .any(|selected| selected == *base_pack)
                    })
                    .map(|pack| (*pack).to_string())
                    .collect();

                let risky = presets::risky_pack_ids(&planned_selection);
                let recommendation = onboard::recommend_security_profile(
                    Some(&intent_text),
                    &planned_selection.packs,
                );
                intent_preview = Some(OnboardIntentPlanPreview {
                    intent: intent_text.clone(),
                    preset: planned_selection.preset_id.clone(),
                    add_packs: plan.add_packs.clone(),
                    remove_packs: plan.remove_packs.clone(),
                    confidence: plan.confidence,
                    risky_packs: risky.clone(),
                    recommended_security_profile_id: recommendation.profile_id.clone(),
                    recommended_security_profile_label: recommendation.label.clone(),
                });

                if !json {
                    println!("Intent plan:");
                    println!("  intent: {intent_text}");
                    println!("  preset: {}", planned_selection.preset_id);
                    if !plan.add_packs.is_empty() {
                        println!("  add: {}", plan.add_packs.join(", "));
                    }
                    if !plan.remove_packs.is_empty() {
                        println!("  remove: {}", plan.remove_packs.join(", "));
                    }
                    println!("  confidence: {:.2}", plan.confidence);
                    if !risky.is_empty() {
                        println!("  risky packs: {}", risky.join(", "));
                    }
                    println!(
                        "  recommended security profile: {} ({})",
                        recommendation.profile_id, recommendation.label
                    );
                }

                if resolved_security_profile.is_none() {
                    resolved_security_profile = Some(recommendation.profile_id.clone());
                }
            }

            let resolved_preset_id = resolved_preset
                .clone()
                .unwrap_or_else(|| "minimal".to_string());
            let base_selection = presets::from_preset_id(&resolved_preset_id)?;
            let planned_selection =
                presets::compose_selection(base_selection, &resolved_pack, &resolved_remove_pack)?;
            let risky = presets::risky_pack_ids(&planned_selection);
            let effective_security_profile = resolved_security_profile
                .clone()
                .unwrap_or_else(|| "strict".to_string());
            let risk_assessment = evaluate_onboard_quick_risk_requirements(
                &risky,
                &effective_security_profile,
                yes_security_risk,
                dry_run,
            )?;

            if dry_run {
                if json {
                    let rebuild_preview = build_rebuild_preview(&planned_selection, rebuild)?;

                    let report = OnboardQuickDryRunReport {
                        schema_version: ONBOARD_QUICK_DRY_RUN_SCHEMA_VERSION,
                        report_type: ONBOARD_QUICK_DRY_RUN_REPORT_TYPE.to_string(),
                        mode: "quick_dry_run".to_string(),
                        intent_plan: intent_preview,
                        planned_selection: planned_selection.clone(),
                        risky_packs: risky.clone(),
                        security_profile: effective_security_profile.clone(),
                        requires_explicit_consent: !risk_assessment.consent_reasons.is_empty(),
                        consent_reasons: risk_assessment.consent_reasons.clone(),
                        consent_reason_keys: consent_reason_keys(&risk_assessment.consent_reasons),
                        warning_codes: risk_assessment.warning_codes.clone(),
                        warning_keys: onboard_warning_keys(&risk_assessment.warning_codes),
                        warnings: risk_assessment.warnings.clone(),
                        rebuild_requested: rebuild,
                        rebuild_preview,
                        config_written: false,
                        workspace_written: false,
                    };
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!("Onboard quick dry-run (no files written):");
                    println!("  preset: {}", planned_selection.preset_id);
                    println!("  packs: {}", planned_selection.packs.join(", "));
                    if !planned_selection.added_packs.is_empty() {
                        println!(
                            "  added packs: {}",
                            planned_selection.added_packs.join(", ")
                        );
                    }
                    if !risky.is_empty() {
                        println!("  risky packs: {}", risky.join(", "));
                    }
                    println!("  security profile: {}", effective_security_profile);
                    for warning in &risk_assessment.warnings {
                        println!("  warning: {warning}");
                    }
                    if rebuild {
                        maybe_rebuild_selection(&planned_selection, true, true, true).await?;
                    }
                }
                return Ok(());
            }

            onboard::run_quick_setup(
                api_key.as_deref(),
                provider.as_deref(),
                model.as_deref(),
                memory.as_deref(),
                resolved_preset.as_deref(),
                &resolved_pack,
                &resolved_remove_pack,
                resolved_security_profile.as_deref(),
                yes_security_risk,
                force,
            )
            .await
        }?;
        let mut run_rebuild = rebuild;
        let mut rebuild_approved = yes_rebuild;

        if !channels_only
            && interactive
            && std::io::stdin().is_terminal()
            && std::io::stdout().is_terminal()
        {
            let has_selection = presets::load_workspace_selection(&config)?.is_some();
            if run_rebuild && !rebuild_approved {
                let confirmed = Confirm::new()
                    .with_prompt("Run rebuild now to compile selected preset/packs?")
                    .default(true)
                    .interact()?;
                if confirmed {
                    rebuild_approved = true;
                } else {
                    run_rebuild = false;
                }
            } else if !run_rebuild && has_selection {
                let run_now = Confirm::new()
                    .with_prompt("Onboarding complete. Rebuild now for selected preset/packs?")
                    .default(false)
                    .interact()?;
                if run_now {
                    run_rebuild = true;
                    rebuild_approved = true;
                }
            }
        }

        if run_rebuild {
            let selection = if let Some(current) = presets::load_workspace_selection(&config)? {
                current
            } else {
                presets::default_selection()?
            };
            maybe_rebuild_selection(&selection, true, false, rebuild_approved).await?;
        }
        // Auto-start channels if user said yes during wizard
        if std::env::var("ZEROCLAW_AUTOSTART_CHANNELS").as_deref() == Ok("1") {
            channels::start_channels(config).await?;
        }
        return Ok(());
    }

    // All other commands need config loaded first
    let mut config = Config::load_or_init().await?;
    config.apply_env_overrides();
    observability::runtime_trace::init_from_config(&config.observability, &config.workspace_dir);
    if config.security.otp.enabled {
        let config_dir = config
            .config_path
            .parent()
            .context("Config path must have a parent directory")?;
        let store = security::SecretStore::new(config_dir, config.secrets.encrypt);
        let (_validator, enrollment_uri) =
            security::OtpValidator::from_config(&config.security.otp, config_dir, &store)?;
        if let Some(uri) = enrollment_uri {
            println!("Initialized OTP secret for ZeroClaw.");
            println!("Enrollment URI: {uri}");
        }
    }

    match cli.command {
        Commands::Onboard { .. } => unreachable!(),
        Commands::Completions { .. } => unreachable!(),

        Commands::Agent {
            message,
            provider,
            model,
            temperature,
            peripheral,
        } => agent::run(
            config,
            message,
            provider,
            model,
            temperature,
            peripheral,
            true,
        )
        .await
        .map(|_| ()),

        Commands::Update {
            apply,
            version,
            install_path,
            dry_run,
            yes,
        } => {
            if apply {
                if !yes && !dry_run {
                    bail!(
                        "Refusing to replace the running binary without explicit confirmation. \
                         Re-run with `--yes`, or add `--dry-run` to preview."
                    );
                }

                let result = updater::apply_update(updater::UpdateApplyOptions {
                    target_version: version,
                    install_path,
                    dry_run,
                })
                .await?;

                if result.dry_run {
                    println!("Update dry-run complete.");
                    println!("  from:          {}", result.from_version);
                    println!("  to:            {}", result.to_version);
                    println!("  target:        {}", result.target);
                    println!("  release asset: {}", result.asset_name);
                    println!("  install path:  {}", result.install_path.display());
                    if let Some(url) = result.release_url {
                        println!("  release:       {url}");
                    }
                } else {
                    println!(
                        "Updated zeroclaw from {} to {}",
                        result.from_version, result.to_version
                    );
                    println!("Installed binary: {}", result.install_path.display());
                }
                Ok(())
            } else {
                let result =
                    updater::check_for_updates(env!("CARGO_PKG_VERSION"), version.as_deref())
                        .await?;

                println!("Current version: {}", result.current_version);
                println!("Latest version:  {}", result.latest_version);
                if result.update_available {
                    println!("Update available: yes");
                    if let Some(url) = &result.release.html_url {
                        println!("Release URL:      {url}");
                    }
                    println!("Run: zeroclaw update --apply --yes");
                } else {
                    println!("Update available: no");
                }

                Ok(())
            }
        }

        Commands::Gateway { port, host } => {
            let port = port.unwrap_or(config.gateway.port);
            let host = host.unwrap_or_else(|| config.gateway.host.clone());
            if port == 0 {
                info!("🚀 Starting ZeroClaw Gateway on {host} (random port)");
            } else {
                info!("🚀 Starting ZeroClaw Gateway on {host}:{port}");
            }
            gateway::run_gateway(&host, port, config).await
        }

        Commands::Daemon { port, host } => {
            let port = port.unwrap_or(config.gateway.port);
            let host = host.unwrap_or_else(|| config.gateway.host.clone());
            if port == 0 {
                info!("🧠 Starting ZeroClaw Daemon on {host} (random port)");
            } else {
                info!("🧠 Starting ZeroClaw Daemon on {host}:{port}");
            }
            daemon::run(config, host, port).await
        }

        Commands::Status => {
            println!("🦀 ZeroClaw Status");
            println!();
            println!("Version:     {}", env!("CARGO_PKG_VERSION"));
            println!("Workspace:   {}", config.workspace_dir.display());
            println!("Config:      {}", config.config_path.display());
            println!();
            println!(
                "🤖 Provider:      {}",
                config.default_provider.as_deref().unwrap_or("openrouter")
            );
            println!(
                "   Model:         {}",
                config.default_model.as_deref().unwrap_or("(default)")
            );
            println!("📊 Observability:  {}", config.observability.backend);
            println!(
                "🧾 Trace storage:  {} ({})",
                config.observability.runtime_trace_mode, config.observability.runtime_trace_path
            );
            println!("🛡️  Autonomy:      {:?}", config.autonomy.level);
            println!("⚙️  Runtime:       {}", config.runtime.kind);
            let effective_memory_backend = memory::effective_memory_backend_name(
                &config.memory.backend,
                Some(&config.storage.provider.config),
            );
            println!(
                "💓 Heartbeat:      {}",
                if config.heartbeat.enabled {
                    format!("every {}min", config.heartbeat.interval_minutes)
                } else {
                    "disabled".into()
                }
            );
            println!(
                "🧠 Memory:         {} (auto-save: {})",
                effective_memory_backend,
                if config.memory.auto_save { "on" } else { "off" }
            );

            println!();
            println!("Security:");
            println!("  Workspace only:    {}", config.autonomy.workspace_only);
            println!(
                "  Allowed roots:     {}",
                if config.autonomy.allowed_roots.is_empty() {
                    "(none)".to_string()
                } else {
                    config.autonomy.allowed_roots.join(", ")
                }
            );
            println!(
                "  Allowed commands:  {}",
                config.autonomy.allowed_commands.join(", ")
            );
            println!(
                "  Max actions/hour:  {}",
                config.autonomy.max_actions_per_hour
            );
            println!(
                "  Max cost/day:      ${:.2}",
                f64::from(config.autonomy.max_cost_per_day_cents) / 100.0
            );
            println!("  OTP enabled:       {}", config.security.otp.enabled);
            println!("  E-stop enabled:    {}", config.security.estop.enabled);
            println!();
            println!("Channels:");
            println!("  CLI:      ✅ always");
            for (channel, configured) in config.channels_config.channels() {
                println!(
                    "  {:9} {}",
                    channel.name(),
                    if configured {
                        "✅ configured"
                    } else {
                        "❌ not configured"
                    }
                );
            }
            println!();
            println!("Peripherals:");
            println!(
                "  Enabled:   {}",
                if config.peripherals.enabled {
                    "yes"
                } else {
                    "no"
                }
            );
            println!("  Boards:    {}", config.peripherals.boards.len());

            Ok(())
        }

        Commands::Estop {
            estop_command,
            level,
            domains,
            tools,
        } => handle_estop_command(&config, estop_command, level, domains, tools),

        Commands::Cron { cron_command } => cron::handle_command(cron_command, &config),

        Commands::Models { model_command } => match model_command {
            ModelCommands::Refresh { provider, force } => {
                onboard::run_models_refresh(&config, provider.as_deref(), force).await
            }
        },

        Commands::Preset { preset_command } => handle_preset_command(preset_command, &config).await,

        Commands::Security { security_command } => {
            handle_security_command(security_command, &mut config).await
        }

        Commands::Providers => {
            let providers = providers::list_providers();
            let current = config
                .default_provider
                .as_deref()
                .unwrap_or("openrouter")
                .trim()
                .to_ascii_lowercase();
            println!("Supported providers ({} total):\n", providers.len());
            println!("  ID (use in config)  DESCRIPTION");
            println!("  ─────────────────── ───────────");
            for p in &providers {
                let is_active = p.name.eq_ignore_ascii_case(&current)
                    || p.aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(&current));
                let marker = if is_active { " (active)" } else { "" };
                let local_tag = if p.local { " [local]" } else { "" };
                let aliases = if p.aliases.is_empty() {
                    String::new()
                } else {
                    format!("  (aliases: {})", p.aliases.join(", "))
                };
                println!(
                    "  {:<19} {}{}{}{}",
                    p.name, p.display_name, local_tag, marker, aliases
                );
            }
            println!("\n  custom:<URL>   Any OpenAI-compatible endpoint");
            println!("  anthropic-custom:<URL>  Any Anthropic-compatible endpoint");
            Ok(())
        }

        Commands::Service {
            service_command,
            service_init,
        } => {
            let init_system = service_init.parse()?;
            service::handle_command(&service_command, &config, init_system)
        }

        Commands::Doctor { doctor_command } => match doctor_command {
            Some(DoctorCommands::Models {
                provider,
                use_cache,
            }) => doctor::run_models(&config, provider.as_deref(), use_cache).await,
            Some(DoctorCommands::Traces {
                id,
                event,
                contains,
                limit,
            }) => doctor::run_traces(
                &config,
                id.as_deref(),
                event.as_deref(),
                contains.as_deref(),
                limit,
            ),
            None => doctor::run(&config),
        },

        Commands::Channel { channel_command } => match channel_command {
            ChannelCommands::Start => channels::start_channels(config).await,
            ChannelCommands::Doctor => channels::doctor_channels(config).await,
            other => channels::handle_command(other, &config).await,
        },

        Commands::Integrations {
            integration_command,
        } => integrations::handle_command(integration_command, &config),

        Commands::Skills { skill_command } => skills::handle_command(skill_command, &config),

        Commands::Migrate { migrate_command } => {
            migration::handle_command(migrate_command, &config).await
        }

        Commands::Memory { memory_command } => {
            memory::cli::handle_command(memory_command, &config).await
        }

        Commands::Auth { auth_command } => handle_auth_command(auth_command, &config).await,

        Commands::Hardware { hardware_command } => {
            hardware::handle_command(hardware_command.clone(), &config)
        }

        Commands::Peripheral { peripheral_command } => {
            peripherals::handle_command(peripheral_command.clone(), &config).await
        }

        Commands::Config { config_command } => match config_command {
            ConfigCommands::Schema => {
                let schema = schemars::schema_for!(config::Config);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&schema).expect("failed to serialize JSON Schema")
                );
                Ok(())
            }
        },
    }
}

fn handle_estop_command(
    config: &Config,
    estop_command: Option<EstopSubcommands>,
    level: Option<EstopLevelArg>,
    domains: Vec<String>,
    tools: Vec<String>,
) -> Result<()> {
    if !config.security.estop.enabled {
        bail!("Emergency stop is disabled. Enable [security.estop].enabled = true in config.toml");
    }

    let config_dir = config
        .config_path
        .parent()
        .context("Config path must have a parent directory")?;
    let mut manager = security::EstopManager::load(&config.security.estop, config_dir)?;

    match estop_command {
        Some(EstopSubcommands::Status) => {
            print_estop_status(&manager.status());
            Ok(())
        }
        Some(EstopSubcommands::Resume {
            network,
            domains,
            tools,
            otp,
        }) => {
            let selector = build_resume_selector(network, domains, tools)?;
            let mut otp_code = otp;
            let otp_validator = if config.security.estop.require_otp_to_resume {
                if !config.security.otp.enabled {
                    bail!(
                        "security.estop.require_otp_to_resume=true but security.otp.enabled=false"
                    );
                }
                if otp_code.is_none() {
                    let entered = Password::new()
                        .with_prompt("Enter OTP code")
                        .allow_empty_password(false)
                        .interact()?;
                    otp_code = Some(entered);
                }

                let store = security::SecretStore::new(config_dir, config.secrets.encrypt);
                let (validator, enrollment_uri) =
                    security::OtpValidator::from_config(&config.security.otp, config_dir, &store)?;
                if let Some(uri) = enrollment_uri {
                    println!("Initialized OTP secret for ZeroClaw.");
                    println!("Enrollment URI: {uri}");
                }
                Some(validator)
            } else {
                None
            };

            manager.resume(selector, otp_code.as_deref(), otp_validator.as_ref())?;
            println!("Estop resume completed.");
            print_estop_status(&manager.status());
            Ok(())
        }
        None => {
            let engage_level = build_engage_level(level, domains, tools)?;
            manager.engage(engage_level)?;
            println!("Estop engaged.");
            print_estop_status(&manager.status());
            Ok(())
        }
    }
}

fn build_engage_level(
    level: Option<EstopLevelArg>,
    domains: Vec<String>,
    tools: Vec<String>,
) -> Result<security::EstopLevel> {
    let requested = level.unwrap_or(EstopLevelArg::KillAll);
    match requested {
        EstopLevelArg::KillAll => {
            if !domains.is_empty() || !tools.is_empty() {
                bail!("--domain/--tool are only valid with --level domain-block/tool-freeze");
            }
            Ok(security::EstopLevel::KillAll)
        }
        EstopLevelArg::NetworkKill => {
            if !domains.is_empty() || !tools.is_empty() {
                bail!("--domain/--tool are not valid with --level network-kill");
            }
            Ok(security::EstopLevel::NetworkKill)
        }
        EstopLevelArg::DomainBlock => {
            if domains.is_empty() {
                bail!("--level domain-block requires at least one --domain");
            }
            if !tools.is_empty() {
                bail!("--tool is not valid with --level domain-block");
            }
            Ok(security::EstopLevel::DomainBlock(domains))
        }
        EstopLevelArg::ToolFreeze => {
            if tools.is_empty() {
                bail!("--level tool-freeze requires at least one --tool");
            }
            if !domains.is_empty() {
                bail!("--domain is not valid with --level tool-freeze");
            }
            Ok(security::EstopLevel::ToolFreeze(tools))
        }
    }
}

fn build_resume_selector(
    network: bool,
    domains: Vec<String>,
    tools: Vec<String>,
) -> Result<security::ResumeSelector> {
    let selected =
        usize::from(network) + usize::from(!domains.is_empty()) + usize::from(!tools.is_empty());
    if selected > 1 {
        bail!("Use only one of --network, --domain, or --tool for estop resume");
    }
    if network {
        return Ok(security::ResumeSelector::Network);
    }
    if !domains.is_empty() {
        return Ok(security::ResumeSelector::Domains(domains));
    }
    if !tools.is_empty() {
        return Ok(security::ResumeSelector::Tools(tools));
    }
    Ok(security::ResumeSelector::KillAll)
}

fn print_estop_status(state: &security::EstopState) {
    println!("Estop status:");
    println!(
        "  engaged:        {}",
        if state.is_engaged() { "yes" } else { "no" }
    );
    println!(
        "  kill_all:       {}",
        if state.kill_all { "active" } else { "inactive" }
    );
    println!(
        "  network_kill:   {}",
        if state.network_kill {
            "active"
        } else {
            "inactive"
        }
    );
    if state.blocked_domains.is_empty() {
        println!("  domain_blocks:  (none)");
    } else {
        println!("  domain_blocks:  {}", state.blocked_domains.join(", "));
    }
    if state.frozen_tools.is_empty() {
        println!("  tool_freeze:    (none)");
    } else {
        println!("  tool_freeze:    {}", state.frozen_tools.join(", "));
    }
    if let Some(updated_at) = &state.updated_at {
        println!("  updated_at:     {updated_at}");
    }
}

fn write_shell_completion<W: Write>(shell: CompletionShell, writer: &mut W) -> Result<()> {
    use clap_complete::generate;
    use clap_complete::shells;

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut cmd, bin_name.clone(), writer),
        CompletionShell::Fish => generate(shells::Fish, &mut cmd, bin_name.clone(), writer),
        CompletionShell::Zsh => generate(shells::Zsh, &mut cmd, bin_name.clone(), writer),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut cmd, bin_name.clone(), writer);
        }
        CompletionShell::Elvish => generate(shells::Elvish, &mut cmd, bin_name, writer),
    }

    writer.flush()?;
    Ok(())
}

// ─── Generic Pending OAuth Login ────────────────────────────────────────────

/// Generic pending OAuth login state, shared across providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingOAuthLogin {
    provider: String,
    profile: String,
    code_verifier: String,
    state: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingOAuthLoginFile {
    #[serde(default)]
    provider: Option<String>,
    profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_verifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    encrypted_code_verifier: Option<String>,
    state: String,
    created_at: String,
}

fn pending_oauth_login_path(config: &Config, provider: &str) -> std::path::PathBuf {
    let filename = format!("auth-{}-pending.json", provider);
    auth::state_dir_from_config(config).join(filename)
}

fn pending_oauth_secret_store(config: &Config) -> security::secrets::SecretStore {
    security::secrets::SecretStore::new(
        &auth::state_dir_from_config(config),
        config.secrets.encrypt,
    )
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

fn save_pending_oauth_login(config: &Config, pending: &PendingOAuthLogin) -> Result<()> {
    let path = pending_oauth_login_path(config, &pending.provider);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let secret_store = pending_oauth_secret_store(config);
    let encrypted_code_verifier = secret_store.encrypt(&pending.code_verifier)?;
    let persisted = PendingOAuthLoginFile {
        provider: Some(pending.provider.clone()),
        profile: pending.profile.clone(),
        code_verifier: None,
        encrypted_code_verifier: Some(encrypted_code_verifier),
        state: pending.state.clone(),
        created_at: pending.created_at.clone(),
    };
    let tmp = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let json = serde_json::to_vec_pretty(&persisted)?;
    std::fs::write(&tmp, json)?;
    set_owner_only_permissions(&tmp)?;
    std::fs::rename(tmp, &path)?;
    set_owner_only_permissions(&path)?;
    Ok(())
}

fn load_pending_oauth_login(config: &Config, provider: &str) -> Result<Option<PendingOAuthLogin>> {
    let path = pending_oauth_login_path(config, provider);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let persisted: PendingOAuthLoginFile = serde_json::from_slice(&bytes)?;
    let secret_store = pending_oauth_secret_store(config);
    let code_verifier = if let Some(encrypted) = persisted.encrypted_code_verifier {
        secret_store.decrypt(&encrypted)?
    } else if let Some(plaintext) = persisted.code_verifier {
        plaintext
    } else {
        bail!("Pending {} login is missing code verifier", provider);
    };
    Ok(Some(PendingOAuthLogin {
        provider: persisted.provider.unwrap_or_else(|| provider.to_string()),
        profile: persisted.profile,
        code_verifier,
        state: persisted.state,
        created_at: persisted.created_at,
    }))
}

fn clear_pending_oauth_login(config: &Config, provider: &str) {
    let path = pending_oauth_login_path(config, provider);
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
        let _ = file.set_len(0);
        let _ = file.sync_all();
    }
    let _ = std::fs::remove_file(path);
}

fn read_auth_input(prompt: &str) -> Result<String> {
    let input = Password::new()
        .with_prompt(prompt)
        .allow_empty_password(false)
        .interact()?;
    Ok(input.trim().to_string())
}

fn read_plain_input(prompt: &str) -> Result<String> {
    let input: String = Input::new().with_prompt(prompt).interact_text()?;
    Ok(input.trim().to_string())
}

fn extract_openai_account_id_for_profile(access_token: &str) -> Option<String> {
    let account_id = auth::openai_oauth::extract_account_id_from_jwt(access_token);
    if account_id.is_none() {
        warn!(
            "Could not extract OpenAI account id from OAuth access token; \
             requests may fail until re-authentication."
        );
    }
    account_id
}

fn format_expiry(profile: &auth::profiles::AuthProfile) -> String {
    match profile
        .token_set
        .as_ref()
        .and_then(|token_set| token_set.expires_at)
    {
        Some(ts) => {
            let now = chrono::Utc::now();
            if ts <= now {
                format!("expired at {}", ts.to_rfc3339())
            } else {
                let mins = (ts - now).num_minutes();
                format!("expires in {mins}m ({})", ts.to_rfc3339())
            }
        }
        None => "n/a".to_string(),
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_auth_command(auth_command: AuthCommands, config: &Config) -> Result<()> {
    let auth_service = auth::AuthService::from_config(config);

    match auth_command {
        AuthCommands::Login {
            provider,
            profile,
            device_code,
        } => {
            let provider = auth::normalize_provider(&provider)?;
            let client = reqwest::Client::new();

            match provider.as_str() {
                "gemini" => {
                    // Gemini OAuth flow
                    if device_code {
                        match auth::gemini_oauth::start_device_code_flow(&client).await {
                            Ok(device) => {
                                println!("Google/Gemini device-code login started.");
                                println!("Visit: {}", device.verification_uri);
                                println!("Code:  {}", device.user_code);
                                if let Some(uri_complete) = &device.verification_uri_complete {
                                    println!("Fast link: {uri_complete}");
                                }

                                let token_set =
                                    auth::gemini_oauth::poll_device_code_tokens(&client, &device)
                                        .await?;
                                let account_id = token_set.id_token.as_deref().and_then(
                                    auth::gemini_oauth::extract_account_email_from_id_token,
                                );

                                auth_service
                                    .store_gemini_tokens(&profile, token_set, account_id, true)
                                    .await?;

                                println!("Saved profile {profile}");
                                println!("Active profile for gemini: {profile}");
                                return Ok(());
                            }
                            Err(e) => {
                                println!(
                                    "Device-code flow unavailable: {e}. Falling back to browser flow."
                                );
                            }
                        }
                    }

                    let pkce = auth::gemini_oauth::generate_pkce_state();
                    let authorize_url = auth::gemini_oauth::build_authorize_url(&pkce)?;

                    // Save pending login for paste-redirect fallback
                    let pending = PendingOAuthLogin {
                        provider: "gemini".to_string(),
                        profile: profile.clone(),
                        code_verifier: pkce.code_verifier.clone(),
                        state: pkce.state.clone(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    save_pending_oauth_login(config, &pending)?;

                    println!("Open this URL in your browser and authorize access:");
                    println!("{authorize_url}");
                    println!();

                    let code = match auth::gemini_oauth::receive_loopback_code(
                        &pkce.state,
                        std::time::Duration::from_secs(180),
                    )
                    .await
                    {
                        Ok(code) => {
                            clear_pending_oauth_login(config, "gemini");
                            code
                        }
                        Err(e) => {
                            println!("Callback capture failed: {e}");
                            println!(
                                "Run `zeroclaw auth paste-redirect --provider gemini --profile {profile}`"
                            );
                            return Ok(());
                        }
                    };

                    let token_set =
                        auth::gemini_oauth::exchange_code_for_tokens(&client, &code, &pkce).await?;
                    let account_id = token_set
                        .id_token
                        .as_deref()
                        .and_then(auth::gemini_oauth::extract_account_email_from_id_token);

                    auth_service
                        .store_gemini_tokens(&profile, token_set, account_id, true)
                        .await?;

                    println!("Saved profile {profile}");
                    println!("Active profile for gemini: {profile}");
                    Ok(())
                }
                "openai-codex" => {
                    // OpenAI Codex OAuth flow
                    if device_code {
                        match auth::openai_oauth::start_device_code_flow(&client).await {
                            Ok(device) => {
                                println!("OpenAI device-code login started.");
                                println!("Visit: {}", device.verification_uri);
                                println!("Code:  {}", device.user_code);
                                if let Some(uri_complete) = &device.verification_uri_complete {
                                    println!("Fast link: {uri_complete}");
                                }
                                if let Some(message) = &device.message {
                                    println!("{message}");
                                }

                                let token_set =
                                    auth::openai_oauth::poll_device_code_tokens(&client, &device)
                                        .await?;
                                let account_id =
                                    extract_openai_account_id_for_profile(&token_set.access_token);

                                auth_service
                                    .store_openai_tokens(&profile, token_set, account_id, true)
                                    .await?;
                                clear_pending_oauth_login(config, "openai");

                                println!("Saved profile {profile}");
                                println!("Active profile for openai-codex: {profile}");
                                return Ok(());
                            }
                            Err(e) => {
                                println!(
                                    "Device-code flow unavailable: {e}. Falling back to browser/paste flow."
                                );
                            }
                        }
                    }

                    let pkce = auth::openai_oauth::generate_pkce_state();
                    let pending = PendingOAuthLogin {
                        provider: "openai".to_string(),
                        profile: profile.clone(),
                        code_verifier: pkce.code_verifier.clone(),
                        state: pkce.state.clone(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    save_pending_oauth_login(config, &pending)?;

                    let authorize_url = auth::openai_oauth::build_authorize_url(&pkce);
                    println!("Open this URL in your browser and authorize access:");
                    println!("{authorize_url}");
                    println!();
                    println!("Waiting for callback at http://localhost:1455/auth/callback ...");

                    let code = match auth::openai_oauth::receive_loopback_code(
                        &pkce.state,
                        std::time::Duration::from_secs(180),
                    )
                    .await
                    {
                        Ok(code) => code,
                        Err(e) => {
                            println!("Callback capture failed: {e}");
                            println!(
                                "Run `zeroclaw auth paste-redirect --provider openai-codex --profile {profile}`"
                            );
                            return Ok(());
                        }
                    };

                    let token_set =
                        auth::openai_oauth::exchange_code_for_tokens(&client, &code, &pkce).await?;
                    let account_id = extract_openai_account_id_for_profile(&token_set.access_token);

                    auth_service
                        .store_openai_tokens(&profile, token_set, account_id, true)
                        .await?;
                    clear_pending_oauth_login(config, "openai");

                    println!("Saved profile {profile}");
                    println!("Active profile for openai-codex: {profile}");
                    Ok(())
                }
                _ => {
                    bail!(
                        "`auth login` supports --provider openai-codex or gemini, got: {provider}"
                    );
                }
            }
        }

        AuthCommands::PasteRedirect {
            provider,
            profile,
            input,
        } => {
            let provider = auth::normalize_provider(&provider)?;

            match provider.as_str() {
                "openai-codex" => {
                    let pending = load_pending_oauth_login(config, "openai")?.ok_or_else(|| {
                        anyhow::anyhow!(
                            "No pending OpenAI login found. Run `zeroclaw auth login --provider openai-codex` first."
                        )
                    })?;

                    if pending.profile != profile {
                        bail!(
                            "Pending login profile mismatch: pending={}, requested={}",
                            pending.profile,
                            profile
                        );
                    }

                    let redirect_input = match input {
                        Some(value) => value,
                        None => read_plain_input("Paste redirect URL or OAuth code")?,
                    };

                    let code = auth::openai_oauth::parse_code_from_redirect(
                        &redirect_input,
                        Some(&pending.state),
                    )?;

                    let pkce = auth::openai_oauth::PkceState {
                        code_verifier: pending.code_verifier.clone(),
                        code_challenge: String::new(),
                        state: pending.state.clone(),
                    };

                    let client = reqwest::Client::new();
                    let token_set =
                        auth::openai_oauth::exchange_code_for_tokens(&client, &code, &pkce).await?;
                    let account_id = extract_openai_account_id_for_profile(&token_set.access_token);

                    auth_service
                        .store_openai_tokens(&profile, token_set, account_id, true)
                        .await?;
                    clear_pending_oauth_login(config, "openai");

                    println!("Saved profile {profile}");
                    println!("Active profile for openai-codex: {profile}");
                }
                "gemini" => {
                    let pending = load_pending_oauth_login(config, "gemini")?.ok_or_else(|| {
                        anyhow::anyhow!(
                            "No pending Gemini login found. Run `zeroclaw auth login --provider gemini` first."
                        )
                    })?;

                    if pending.profile != profile {
                        bail!(
                            "Pending login profile mismatch: pending={}, requested={}",
                            pending.profile,
                            profile
                        );
                    }

                    let redirect_input = match input {
                        Some(value) => value,
                        None => read_plain_input("Paste redirect URL or OAuth code")?,
                    };

                    let code = auth::gemini_oauth::parse_code_from_redirect(
                        &redirect_input,
                        Some(&pending.state),
                    )?;

                    let pkce = auth::gemini_oauth::PkceState {
                        code_verifier: pending.code_verifier.clone(),
                        code_challenge: String::new(),
                        state: pending.state.clone(),
                    };

                    let client = reqwest::Client::new();
                    let token_set =
                        auth::gemini_oauth::exchange_code_for_tokens(&client, &code, &pkce).await?;
                    let account_id = token_set
                        .id_token
                        .as_deref()
                        .and_then(auth::gemini_oauth::extract_account_email_from_id_token);

                    auth_service
                        .store_gemini_tokens(&profile, token_set, account_id, true)
                        .await?;
                    clear_pending_oauth_login(config, "gemini");

                    println!("Saved profile {profile}");
                    println!("Active profile for gemini: {profile}");
                }
                _ => {
                    bail!("`auth paste-redirect` supports --provider openai-codex or gemini");
                }
            }
            Ok(())
        }

        AuthCommands::PasteToken {
            provider,
            profile,
            token,
            auth_kind,
        } => {
            let provider = auth::normalize_provider(&provider)?;
            let token = match token {
                Some(token) => token.trim().to_string(),
                None => read_auth_input("Paste token")?,
            };
            if token.is_empty() {
                bail!("Token cannot be empty");
            }

            let kind = auth::anthropic_token::detect_auth_kind(&token, auth_kind.as_deref());
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "auth_kind".to_string(),
                kind.as_metadata_value().to_string(),
            );

            auth_service
                .store_provider_token(&provider, &profile, &token, metadata, true)
                .await?;
            println!("Saved profile {profile}");
            println!("Active profile for {provider}: {profile}");
            Ok(())
        }

        AuthCommands::SetupToken { provider, profile } => {
            let provider = auth::normalize_provider(&provider)?;
            let token = read_auth_input("Paste token")?;
            if token.is_empty() {
                bail!("Token cannot be empty");
            }

            let kind = auth::anthropic_token::detect_auth_kind(&token, Some("authorization"));
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "auth_kind".to_string(),
                kind.as_metadata_value().to_string(),
            );

            auth_service
                .store_provider_token(&provider, &profile, &token, metadata, true)
                .await?;
            println!("Saved profile {profile}");
            println!("Active profile for {provider}: {profile}");
            Ok(())
        }

        AuthCommands::Refresh { provider, profile } => {
            let provider = auth::normalize_provider(&provider)?;

            match provider.as_str() {
                "openai-codex" => {
                    match auth_service
                        .get_valid_openai_access_token(profile.as_deref())
                        .await?
                    {
                        Some(_) => {
                            println!("OpenAI Codex token is valid (refresh completed if needed).");
                            Ok(())
                        }
                        None => {
                            bail!(
                                "No OpenAI Codex auth profile found. Run `zeroclaw auth login --provider openai-codex`."
                            )
                        }
                    }
                }
                "gemini" => {
                    match auth_service
                        .get_valid_gemini_access_token(profile.as_deref())
                        .await?
                    {
                        Some(_) => {
                            let profile_name = profile.as_deref().unwrap_or("default");
                            println!("✓ Gemini token refreshed successfully");
                            println!("  Profile: gemini:{}", profile_name);
                            Ok(())
                        }
                        None => {
                            bail!(
                                "No Gemini auth profile found. Run `zeroclaw auth login --provider gemini`."
                            )
                        }
                    }
                }
                _ => bail!("`auth refresh` supports --provider openai-codex or gemini"),
            }
        }

        AuthCommands::Logout { provider, profile } => {
            let provider = auth::normalize_provider(&provider)?;
            let removed = auth_service.remove_profile(&provider, &profile).await?;
            if removed {
                println!("Removed auth profile {provider}:{profile}");
            } else {
                println!("Auth profile not found: {provider}:{profile}");
            }
            Ok(())
        }

        AuthCommands::Use { provider, profile } => {
            let provider = auth::normalize_provider(&provider)?;
            auth_service.set_active_profile(&provider, &profile).await?;
            println!("Active profile for {provider}: {profile}");
            Ok(())
        }

        AuthCommands::List => {
            let data = auth_service.load_profiles().await?;
            if data.profiles.is_empty() {
                println!("No auth profiles configured.");
                return Ok(());
            }

            for (id, profile) in &data.profiles {
                let active = data
                    .active_profiles
                    .get(&profile.provider)
                    .is_some_and(|active_id| active_id == id);
                let marker = if active { "*" } else { " " };
                println!("{marker} {id}");
            }

            Ok(())
        }

        AuthCommands::Status => {
            let data = auth_service.load_profiles().await?;
            if data.profiles.is_empty() {
                println!("No auth profiles configured.");
                return Ok(());
            }

            for (id, profile) in &data.profiles {
                let active = data
                    .active_profiles
                    .get(&profile.provider)
                    .is_some_and(|active_id| active_id == id);
                let marker = if active { "*" } else { " " };
                println!(
                    "{} {} kind={:?} account={} expires={}",
                    marker,
                    id,
                    profile.kind,
                    crate::security::redact(profile.account_id.as_deref().unwrap_or("unknown")),
                    format_expiry(profile)
                );
            }

            println!();
            println!("Active profiles:");
            for (provider, profile_id) in &data.active_profiles {
                println!("  {provider}: {profile_id}");
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn cli_definition_has_no_flag_conflicts() {
        Cli::command().debug_assert();
    }

    #[test]
    fn onboard_help_includes_model_flag() {
        let cmd = Cli::command();
        let onboard = cmd
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "onboard")
            .expect("onboard subcommand must exist");

        let has_model_flag = onboard
            .get_arguments()
            .any(|arg| arg.get_id().as_str() == "model" && arg.get_long() == Some("model"));

        assert!(
            has_model_flag,
            "onboard help should include --model for quick setup overrides"
        );
    }

    #[test]
    fn onboard_cli_accepts_model_provider_and_api_key_in_quick_mode() {
        let cli = Cli::try_parse_from([
            "zeroclaw",
            "onboard",
            "--provider",
            "openrouter",
            "--model",
            "custom-model-946",
            "--api-key",
            "sk-issue946",
        ])
        .expect("quick onboard invocation should parse");

        match cli.command {
            Commands::Onboard {
                interactive,
                force,
                channels_only,
                api_key,
                provider,
                model,
                ..
            } => {
                assert!(!interactive);
                assert!(!force);
                assert!(!channels_only);
                assert_eq!(provider.as_deref(), Some("openrouter"));
                assert_eq!(model.as_deref(), Some("custom-model-946"));
                assert_eq!(api_key.as_deref(), Some("sk-issue946"));
            }
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn onboard_cli_accepts_preset_pack_security_and_rebuild_flags() {
        let cli = Cli::try_parse_from([
            "zeroclaw",
            "onboard",
            "--preset",
            "automation",
            "--pack",
            "rag-pdf",
            "--security-profile",
            "flexible",
            "--yes-security-risk",
            "--rebuild",
            "--yes-rebuild",
        ])
        .expect("onboard preset/pack/security/rebuild invocation should parse");

        match cli.command {
            Commands::Onboard {
                preset,
                pack,
                security_profile,
                yes_security_risk,
                rebuild,
                yes_rebuild,
                ..
            } => {
                assert_eq!(preset.as_deref(), Some("automation"));
                assert_eq!(pack, vec!["rag-pdf".to_string()]);
                assert_eq!(security_profile, Some(SecurityProfileArg::Flexible));
                assert!(yes_security_risk);
                assert!(rebuild);
                assert!(yes_rebuild);
            }
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn onboard_cli_accepts_intent_flag() {
        let cli = Cli::try_parse_from([
            "zeroclaw",
            "onboard",
            "--intent",
            "need browser automation but no update",
            "--provider",
            "openrouter",
        ])
        .expect("onboard intent invocation should parse");

        match cli.command {
            Commands::Onboard {
                intent,
                preset,
                pack,
                ..
            } => {
                assert_eq!(
                    intent.as_deref(),
                    Some("need browser automation but no update")
                );
                assert!(preset.is_none());
                assert!(pack.is_empty());
            }
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn onboard_cli_accepts_dry_run_with_intent_and_rebuild() {
        let cli = Cli::try_parse_from([
            "zeroclaw",
            "onboard",
            "--intent",
            "need browser automation but no update",
            "--dry-run",
            "--rebuild",
        ])
        .expect("onboard dry-run intent invocation should parse");

        match cli.command {
            Commands::Onboard {
                intent,
                dry_run,
                rebuild,
                yes_rebuild,
                ..
            } => {
                assert_eq!(
                    intent.as_deref(),
                    Some("need browser automation but no update")
                );
                assert!(dry_run);
                assert!(rebuild);
                assert!(!yes_rebuild);
            }
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn onboard_cli_accepts_json_in_dry_run_mode() {
        let cli = Cli::try_parse_from([
            "zeroclaw",
            "onboard",
            "--intent",
            "need browser automation but no update",
            "--dry-run",
            "--json",
        ])
        .expect("onboard dry-run json invocation should parse");

        match cli.command {
            Commands::Onboard {
                dry_run,
                json,
                intent,
                ..
            } => {
                assert!(dry_run);
                assert!(json);
                assert_eq!(
                    intent.as_deref(),
                    Some("need browser automation but no update")
                );
            }
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn machine_json_mode_detects_supported_json_commands() {
        let cases: Vec<(Vec<&str>, bool)> = vec![
            (
                vec![
                    "zeroclaw",
                    "onboard",
                    "--intent",
                    "need browser automation",
                    "--dry-run",
                    "--json",
                ],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "intent",
                    "need unattended browser automation",
                    "--json",
                ],
                true,
            ),
            (
                vec!["zeroclaw", "preset", "apply", "--dry-run", "--json"],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "import",
                    "presets/community/template.preset.json",
                    "--dry-run",
                    "--json",
                ],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "export",
                    "/tmp/zeroclaw-export.json",
                    "--json",
                ],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "validate",
                    "presets/official/minimal.toml",
                    "--json",
                ],
                true,
            ),
            (vec!["zeroclaw", "preset", "apply", "--dry-run"], false),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "import",
                    "presets/community/template.preset.json",
                    "--dry-run",
                ],
                false,
            ),
            (
                vec!["zeroclaw", "preset", "export", "/tmp/zeroclaw-export.json"],
                false,
            ),
            (
                vec![
                    "zeroclaw",
                    "security",
                    "profile",
                    "set",
                    "strict",
                    "--dry-run",
                    "--json",
                ],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "security",
                    "profile",
                    "recommend",
                    "need unattended browser automation",
                    "--json",
                ],
                true,
            ),
            (
                vec![
                    "zeroclaw",
                    "onboard",
                    "--intent",
                    "need browser automation",
                    "--dry-run",
                ],
                false,
            ),
            (
                vec![
                    "zeroclaw",
                    "preset",
                    "intent",
                    "need unattended browser automation",
                ],
                false,
            ),
            (
                vec![
                    "zeroclaw",
                    "security",
                    "profile",
                    "set",
                    "strict",
                    "--dry-run",
                ],
                false,
            ),
            (vec!["zeroclaw", "completions", "bash"], false),
        ];

        for (args, expected_machine_json) in cases {
            let cli = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("failed to parse args {:?}: {error}", args));
            assert_eq!(
                command_requests_machine_json(&cli.command),
                expected_machine_json,
                "unexpected machine-json mode for args {:?}",
                args
            );
        }
    }

    #[test]
    fn onboard_runtime_validation_rejects_intent_with_interactive_mode() {
        let error = validate_onboard_command_mode(
            true,  // interactive
            false, // channels_only
            false, // force
            false, // api_key_present
            false, // provider_present
            false, // model_present
            false, // memory_present
            false, // preset_present
            false, // pack_present
            true,  // intent_present
            false, // security_profile_present
            false, // yes_security_risk
            false, // dry_run
            false, // json
            false, // rebuild
            false, // yes_rebuild
        )
        .expect_err("intent should not be accepted with --interactive");
        assert!(
            error
                .to_string()
                .contains("`--intent` is supported in quick onboard mode only"),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn onboard_runtime_validation_rejects_channels_only_with_quick_flags() {
        let error = validate_onboard_command_mode(
            false, // interactive
            true,  // channels_only
            false, // force
            false, // api_key_present
            true,  // provider_present
            false, // model_present
            false, // memory_present
            false, // preset_present
            false, // pack_present
            false, // intent_present
            false, // security_profile_present
            false, // yes_security_risk
            false, // dry_run
            false, // json
            false, // rebuild
            false, // yes_rebuild
        )
        .expect_err("--channels-only should reject quick flags");
        assert!(
            error
                .to_string()
                .contains("--channels-only does not accept quick-setup or rebuild flags"),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn onboard_runtime_validation_requires_yes_rebuild_with_rebuild_flag() {
        let error = validate_onboard_command_mode(
            false, // interactive
            false, // channels_only
            false, // force
            false, // api_key_present
            false, // provider_present
            false, // model_present
            false, // memory_present
            false, // preset_present
            false, // pack_present
            false, // intent_present
            false, // security_profile_present
            false, // yes_security_risk
            false, // dry_run
            false, // json
            false, // rebuild
            true,  // yes_rebuild
        )
        .expect_err("--yes-rebuild without --rebuild should fail");
        assert!(
            error
                .to_string()
                .contains("`--yes-rebuild` requires `--rebuild`."),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn onboard_runtime_validation_requires_dry_run_for_json_mode() {
        let error = validate_onboard_command_mode(
            false, // interactive
            false, // channels_only
            false, // force
            false, // api_key_present
            false, // provider_present
            false, // model_present
            false, // memory_present
            false, // preset_present
            false, // pack_present
            false, // intent_present
            false, // security_profile_present
            false, // yes_security_risk
            false, // dry_run
            true,  // json
            false, // rebuild
            false, // yes_rebuild
        )
        .expect_err("onboard --json should require --dry-run");
        assert!(
            error
                .to_string()
                .contains("`--json` requires `--dry-run` in onboard quick mode."),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn onboard_quick_risk_requirements_reject_risky_pack_without_consent() {
        let error = evaluate_onboard_quick_risk_requirements(
            &["tools-update".to_string()],
            "strict",
            false,
            false,
        )
        .expect_err("non-dry-run risky selection should require explicit consent");
        assert!(
            error
                .to_string()
                .contains("Selection includes risky packs [tools-update]"),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn onboard_quick_risk_requirements_warn_in_dry_run_without_consent() {
        let assessment = evaluate_onboard_quick_risk_requirements(
            &["tools-update".to_string()],
            "balanced",
            false,
            true,
        )
        .expect("dry-run should allow preview without explicit consent");
        assert_eq!(assessment.warnings.len(), 2);
        assert_eq!(
            assessment.consent_reasons,
            vec![
                ConsentReasonCode::RiskyPack,
                ConsentReasonCode::SecurityNonStrict
            ]
        );
        assert_eq!(
            assessment.warning_codes,
            vec![
                OnboardWarningCode::RiskyPackRequiresConsent,
                OnboardWarningCode::SecurityNonStrictRequiresConsent
            ]
        );
        assert!(
            assessment
                .warnings
                .iter()
                .any(|line| line.contains("risky packs [tools-update]")),
            "missing risky pack warning: {:?}",
            assessment.warnings
        );
        assert!(
            assessment
                .warnings
                .iter()
                .any(|line| line.contains("Security profile 'balanced' is non-strict")),
            "missing security profile warning: {:?}",
            assessment.warnings
        );
    }

    #[test]
    fn onboard_quick_risk_requirements_allow_consented_selection() {
        let assessment = evaluate_onboard_quick_risk_requirements(
            &["tools-update".to_string()],
            "balanced",
            true,
            false,
        )
        .expect("consented selection should pass");
        assert!(
            assessment.warnings.is_empty(),
            "warnings should be empty when consent is provided: {:?}",
            assessment.warnings
        );
        assert!(
            assessment.consent_reasons.is_empty(),
            "consent reasons should be empty when consent is provided: {:?}",
            assessment.consent_reasons
        );
        assert!(
            assessment.warning_codes.is_empty(),
            "warning codes should be empty when consent is provided: {:?}",
            assessment.warning_codes
        );
    }

    #[test]
    fn consent_reason_keys_are_stable_and_ordered() {
        let keys = consent_reason_keys(&[
            ConsentReasonCode::RiskyPack,
            ConsentReasonCode::Rebuild,
            ConsentReasonCode::SecurityNonStrict,
        ]);
        assert_eq!(
            keys,
            vec![
                "consent.reason.risky_pack".to_string(),
                "consent.reason.rebuild".to_string(),
                "consent.reason.security_non_strict".to_string()
            ]
        );
    }

    #[test]
    fn onboard_warning_keys_are_stable_and_ordered() {
        let keys = onboard_warning_keys(&[
            OnboardWarningCode::RiskyPackRequiresConsent,
            OnboardWarningCode::SecurityNonStrictRequiresConsent,
        ]);
        assert_eq!(
            keys,
            vec![
                "onboard.warning.risky_pack_requires_consent".to_string(),
                "onboard.warning.security_non_strict_requires_consent".to_string()
            ]
        );
    }

    #[test]
    fn security_risk_consent_reasons_and_keys_are_stable_and_ordered() {
        let reasons = build_security_risk_consent_reasons(true, true);
        assert_eq!(
            reasons,
            vec![
                SecurityRiskConsentReasonCode::NonStrictProfile,
                SecurityRiskConsentReasonCode::NonCliAutoApproval
            ]
        );
        let keys = security_risk_consent_reason_keys(&reasons);
        assert_eq!(
            keys,
            vec![
                "security.risk_reason.non_strict_profile".to_string(),
                "security.risk_reason.non_cli_auto_approval".to_string()
            ]
        );
    }

    #[test]
    fn security_profile_change_report_contains_schema_and_reason_keys() {
        let current = onboard::autonomy_config_for_security_profile_id("strict")
            .expect("strict profile should exist");
        let target = onboard::autonomy_config_for_security_profile_id("balanced")
            .expect("balanced profile should exist");
        let reasons = build_security_risk_consent_reasons(true, false);
        let report =
            build_security_profile_change_report(&current, &target, "balanced", &reasons, true);
        assert_eq!(
            report.schema_version,
            SECURITY_PROFILE_CHANGE_SCHEMA_VERSION
        );
        assert_eq!(report.report_type, SECURITY_PROFILE_CHANGE_REPORT_TYPE);
        assert!(report.requires_explicit_risk_consent);
        assert_eq!(report.risk_consent_reasons, reasons);
        assert_eq!(
            report.risk_consent_reason_keys,
            vec!["security.risk_reason.non_strict_profile".to_string()]
        );
    }

    #[test]
    fn machine_json_report_type_constants_are_stable() {
        assert_eq!(ONBOARD_QUICK_DRY_RUN_REPORT_TYPE, "onboard.quick_dry_run");
        assert_eq!(
            PRESET_INTENT_ORCHESTRATION_REPORT_TYPE,
            "preset.intent_orchestration"
        );
        assert_eq!(PRESET_APPLY_DRY_RUN_REPORT_TYPE, "preset.apply_dry_run");
        assert_eq!(PRESET_IMPORT_DRY_RUN_REPORT_TYPE, "preset.import_dry_run");
        assert_eq!(PRESET_EXPORT_REPORT_TYPE, "preset.export");
        assert_eq!(
            SECURITY_PROFILE_CHANGE_REPORT_TYPE,
            "security.profile_change"
        );
        assert_eq!(
            SECURITY_PROFILE_RECOMMEND_REPORT_TYPE,
            "security.profile_recommendation"
        );
    }

    #[test]
    fn completions_cli_parses_supported_shells() {
        for shell in ["bash", "fish", "zsh", "powershell", "elvish"] {
            let cli = Cli::try_parse_from(["zeroclaw", "completions", shell])
                .expect("completions invocation should parse");
            match cli.command {
                Commands::Completions { .. } => {}
                other => panic!("expected completions command, got {other:?}"),
            }
        }
    }

    #[test]
    fn completion_generation_mentions_binary_name() {
        let mut output = Vec::new();
        write_shell_completion(CompletionShell::Bash, &mut output)
            .expect("completion generation should succeed");
        let script = String::from_utf8(output).expect("completion output should be valid utf-8");
        assert!(
            script.contains("zeroclaw"),
            "completion script should reference binary name"
        );
    }

    #[test]
    fn onboard_cli_accepts_force_flag() {
        let cli = Cli::try_parse_from(["zeroclaw", "onboard", "--force"])
            .expect("onboard --force should parse");

        match cli.command {
            Commands::Onboard { force, .. } => assert!(force),
            other => panic!("expected onboard command, got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_estop_default_engage() {
        let cli = Cli::try_parse_from(["zeroclaw", "estop"]).expect("estop command should parse");

        match cli.command {
            Commands::Estop {
                estop_command,
                level,
                domains,
                tools,
            } => {
                assert!(estop_command.is_none());
                assert!(level.is_none());
                assert!(domains.is_empty());
                assert!(tools.is_empty());
            }
            other => panic!("expected estop command, got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_estop_resume_domain() {
        let cli = Cli::try_parse_from(["zeroclaw", "estop", "resume", "--domain", "*.chase.com"])
            .expect("estop resume command should parse");

        match cli.command {
            Commands::Estop {
                estop_command: Some(EstopSubcommands::Resume { domains, .. }),
                ..
            } => assert_eq!(domains, vec!["*.chase.com".to_string()]),
            other => panic!("expected estop resume command, got {other:?}"),
        }
    }
}
