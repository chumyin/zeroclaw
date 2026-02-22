# ZeroClaw Commands Reference

This reference is derived from the current CLI surface (`zeroclaw --help`).

Last verified: **February 22, 2026**.

## Top-Level Commands

| Command | Purpose |
|---|---|
| `onboard` | Initialize workspace/config quickly or interactively |
| `agent` | Run interactive chat or single-message mode |
| `update` | Check/apply binary updates from GitHub Releases |
| `gateway` | Start webhook and WhatsApp HTTP gateway |
| `daemon` | Start supervised runtime (gateway + channels + optional heartbeat/scheduler) |
| `service` | Manage user-level OS service lifecycle |
| `doctor` | Run diagnostics and freshness checks |
| `status` | Print current configuration and system summary |
| `estop` | Engage/resume emergency stop levels and inspect estop state |
| `cron` | Manage scheduled tasks |
| `models` | Refresh provider model catalogs |
| `preset` | Manage preset composition/import/export/intent planning |
| `security` | Inspect and change security/autonomy profiles |
| `providers` | List provider IDs, aliases, and active provider |
| `channel` | Manage channels and channel health checks |
| `integrations` | Inspect integration details |
| `skills` | List/install/remove skills |
| `migrate` | Import from external runtimes (currently OpenClaw) |
| `config` | Export machine-readable config schema |
| `completions` | Generate shell completion scripts to stdout |
| `hardware` | Discover and introspect USB hardware |
| `peripheral` | Configure and flash peripherals |

## Command Groups

### `onboard`

- `zeroclaw onboard`
- `zeroclaw onboard --interactive`
- `zeroclaw onboard --channels-only`
- `zeroclaw onboard --force`
- `zeroclaw onboard --api-key <KEY> --provider <ID> --memory <sqlite|lucid|markdown|none>`
- `zeroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none>`
- `zeroclaw onboard --preset <minimal|default|automation|hardware-lab|hardened-linux> [--pack <PACK>]...`
- `zeroclaw onboard --intent "<natural language requirements>"`
- `zeroclaw onboard --intent "<natural language requirements>" --dry-run [--rebuild]`
- `zeroclaw onboard --intent "<natural language requirements>" --dry-run --json`
- `zeroclaw onboard --security-profile <strict|balanced|flexible|full> [--yes-security-risk]`
- `zeroclaw onboard --rebuild --yes-rebuild`
- `zeroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none> --force`

`onboard` safety behavior:

- If `config.toml` already exists and you run `--interactive`, onboarding now offers two modes:
  - Full onboarding (overwrite `config.toml`)
  - Provider-only update (update provider/model/API key while preserving existing channels, tunnel, memory, hooks, and other settings)
- In non-interactive environments, existing `config.toml` causes a safe refusal unless `--force` is passed.
- Quick mode defaults to the `minimal` preset (core-first, no risky packs).
- `--intent` uses built-in intent rules to map natural-language requirements to preset/packs in quick mode.
- `--dry-run` previews quick onboarding composition without writing `config.toml` or workspace scaffold files.
- `onboard --json` is quick dry-run only and requires `--dry-run`; it is intended for machine consumers.
- Applying risky packs (for example `tools-update`) or a non-strict security profile requires explicit `--yes-security-risk` consent in quick mode.
- `--dry-run` can preview risky/non-strict plans without consent and prints warnings describing what would require confirmation at apply time.
- `onboard --dry-run --json` includes `schema_version` for parser compatibility and stable reason fields for UI/agent flows:
  - `report_type`: `onboard.quick_dry_run`
  - `consent_reasons`: `risky_pack`, `security_non_strict`
  - `consent_reason_keys`: `consent.reason.risky_pack`, `consent.reason.security_non_strict`
  - `warning_codes`: `risky_pack_requires_consent`, `security_non_strict_requires_consent`
  - `warning_keys`: `onboard.warning.risky_pack_requires_consent`, `onboard.warning.security_non_strict_requires_consent`
- Rebuild execution requires both `--rebuild` and `--yes-rebuild`.
- In interactive onboard sessions, the wizard can prompt to run rebuild immediately after setup.
- Use `zeroclaw onboard --channels-only` when you only need to rotate channel tokens/allowlists.

### `agent`

- `zeroclaw agent`
- `zeroclaw agent -m "Hello"`
- `zeroclaw agent --provider <ID> --model <MODEL> --temperature <0.0-2.0>`
- `zeroclaw agent --peripheral <board:path>`

Tip:

- In interactive chat, you can ask for route changes in natural language (for example “conversation uses kimi, coding uses gpt-5.3-codex”); the assistant can persist this via tool `model_routing_config`.

### `gateway` / `daemon`

- `zeroclaw gateway [--host <HOST>] [--port <PORT>]`
- `zeroclaw daemon [--host <HOST>] [--port <PORT>]`

### `estop`

- `zeroclaw estop` (engage `kill-all`)
- `zeroclaw estop --level network-kill`
- `zeroclaw estop --level domain-block --domain "*.chase.com" [--domain "*.paypal.com"]`
- `zeroclaw estop --level tool-freeze --tool shell [--tool browser]`
- `zeroclaw estop status`
- `zeroclaw estop resume`
- `zeroclaw estop resume --network`
- `zeroclaw estop resume --domain "*.chase.com"`
- `zeroclaw estop resume --tool shell`
- `zeroclaw estop resume --otp <123456>`

Notes:

- `estop` commands require `[security.estop].enabled = true`.
- When `[security.estop].require_otp_to_resume = true`, `resume` requires OTP validation.
- OTP prompt appears automatically if `--otp` is omitted.

### `service`

- `zeroclaw service install`
- `zeroclaw service start`
- `zeroclaw service stop`
- `zeroclaw service restart`
- `zeroclaw service status`
- `zeroclaw service uninstall`

### `cron`

- `zeroclaw cron list`
- `zeroclaw cron add <expr> [--tz <IANA_TZ>] <command>`
- `zeroclaw cron add-at <rfc3339_timestamp> <command>`
- `zeroclaw cron add-every <every_ms> <command>`
- `zeroclaw cron once <delay> <command>`
- `zeroclaw cron remove <id>`
- `zeroclaw cron pause <id>`
- `zeroclaw cron resume <id>`

Notes:

- Mutating schedule/cron actions require `cron.enabled = true`.
- Shell command payloads for schedule creation (`create` / `add` / `once`) are validated by security command policy before job persistence.

### `models`

- `zeroclaw models refresh`
- `zeroclaw models refresh --provider <ID>`
- `zeroclaw models refresh --force`

`models refresh` currently supports live catalog refresh for provider IDs: `openrouter`, `openai`, `anthropic`, `groq`, `mistral`, `deepseek`, `xai`, `together-ai`, `gemini`, `ollama`, `llamacpp`, `sglang`, `vllm`, `astrai`, `venice`, `fireworks`, `cohere`, `moonshot`, `glm`, `zai`, `qwen`, and `nvidia`.

### `doctor`

- `zeroclaw doctor`
- `zeroclaw doctor models [--provider <ID>] [--use-cache]`
- `zeroclaw doctor traces [--limit <N>] [--event <TYPE>] [--contains <TEXT>]`
- `zeroclaw doctor traces --id <TRACE_ID>`

`doctor traces` reads runtime tool/model diagnostics from `observability.runtime_trace_path`.

### `preset`

- `zeroclaw preset list`
- `zeroclaw preset show <ID>`
- `zeroclaw preset current`
- `zeroclaw preset apply [--preset <ID>] [--pack <PACK>]... [--remove-pack <PACK>]... [--dry-run] [--yes-risky] [--rebuild --yes-rebuild] [--json]`
- `zeroclaw preset intent "<text>" [--capabilities-file <path>]...` (plan only)
- `zeroclaw preset intent "<text>" --json [--capabilities-file <path>]...` (plan + security recommendation + generated next commands, no write)
- `zeroclaw preset intent "<text>" --emit-shell <path> [--capabilities-file <path>]...` (write orchestration script template, no execute)
- `zeroclaw preset intent "<text>" --apply [--capabilities-file <path>]... [--dry-run] [--yes-risky] [--rebuild --yes-rebuild]`
- `zeroclaw preset export <path> [--preset <ID>] [--json]`
- `zeroclaw preset import <path> [--mode overwrite|merge|fill] [--dry-run] [--yes-risky] [--rebuild --yes-rebuild] [--json]`
- `zeroclaw preset validate <path...> [--allow-unknown-packs] [--json]`
- `zeroclaw preset rebuild [--dry-run] [--yes]`

Safety notes:

- Risk-gated packs require explicit approval with `--yes-risky` when applying/importing/intent-applying.
- Rebuild execution requires explicit approval (`--yes-rebuild` for apply/import/intent and `--yes` for `preset rebuild`).
- `preset apply --json` and `preset import --json` are machine dry-run previews only and require `--dry-run`.
- `preset export --json` emits a machine-readable write report (`preset.export`) and still writes the export payload file.
- `preset intent --json` is advisory/orchestration mode only and cannot be combined with `--apply`.
- `preset intent --emit-shell` is advisory/orchestration mode only and cannot be combined with `--apply`.
- `preset intent` in plan mode prints generated follow-up commands but does not execute them.
- `preset apply --dry-run --json` includes `schema_version`, `report_type` (`preset.apply_dry_run`), `selection_diff`, risky-pack consent fields (`apply_consent_reasons`, `apply_consent_reason_keys`), and optional `rebuild_preview`.
- `preset import --dry-run --json` includes `schema_version`, `report_type` (`preset.import_dry_run`), import metadata (`import_mode`, `source_path`), `selection_diff`, risky-pack consent fields, and optional `rebuild_preview`.
- `preset export --json` includes `schema_version`, `report_type` (`preset.export`), export provenance (`source_kind`, `requested_preset`), and integrity metadata (`target_path`, `bytes_written`, `payload_sha256`).
- `preset intent --json` includes `schema_version`, `report_type` (`preset.intent_orchestration`), plus `next_commands[].consent_reasons` and `next_commands[].consent_reason_keys` for UI/agent confirmation flows (for example `risky_pack` + `consent.reason.risky_pack`).
- `preset validate --json` includes `schema_version`, `report_type` (`preset.validation`), and per-file structured results suitable for CI/automation pipelines.
- For field-level compatibility guarantees and integration guidance, see [preset-machine-contract.md](preset-machine-contract.md).

### `security`

- `zeroclaw security show`
- `zeroclaw security profile set strict`
- `zeroclaw security profile set balanced --dry-run`
- `zeroclaw security profile set flexible --yes-risk`
- `zeroclaw security profile set full --yes-risk`
- `zeroclaw security profile set strict --non-cli-approval manual`
- `zeroclaw security profile set strict --non-cli-approval auto --yes-risk`
- `zeroclaw security profile recommend "need unattended browser automation"`
- `zeroclaw security profile recommend "need unattended browser automation" --from-preset automation --pack rag-pdf`
- `zeroclaw security profile recommend "hardened deployment" --from-preset hardened-linux --remove-pack tools-update`
- `zeroclaw security profile set full --dry-run --json`
- `zeroclaw security profile set balanced --dry-run --export-diff .zeroclaw-security-diff.json`

Safety notes:

- Setting non-strict profiles requires explicit consent (`--yes-risk`) unless using `--dry-run`.
- Enabling non-CLI auto-approval (`--non-cli-approval auto`) also requires explicit consent (`--yes-risk`) unless using `--dry-run`.
- `--non-cli-approval manual|auto` controls whether non-CLI channels can auto-approve approval-gated tool calls.
- `onboard --security-profile` in quick mode also requires `--yes-security-risk` for non-strict profiles.
- `security profile set` supports machine-readable reports via `--json` and file export via `--export-diff <PATH>`.
- `security profile recommend` is advisory-only (no write). Use it to turn intent text + preset plan into a guarded profile suggestion.
- `security profile recommend` supports preflight composition via `--from-preset`, `--pack`, and `--remove-pack` without mutating workspace state.
- `security profile set --json` includes `schema_version`, `report_type` (`security.profile_change`), and structured consent reasons (`risk_consent_reasons`, `risk_consent_reason_keys`).
- `security profile recommend --json` includes `schema_version`, `report_type` (`security.profile_recommendation`), and apply-step consent fields (`apply_requires_explicit_risk_consent`, `apply_consent_reasons`, `apply_consent_reason_keys`).
- If you need to immediately return to safe defaults, run `zeroclaw security profile set strict`.
- After onboarding, agent tool calls cannot silently bypass policy guards. If an operation is blocked by security policy, tool results include remediation guidance (`security show`, `security profile recommend`, and graded `security profile set ... --yes-risk` options) plus explicit risk warnings.

### `channel`

- `zeroclaw channel list`
- `zeroclaw channel start`
- `zeroclaw channel doctor`
- `zeroclaw channel bind-telegram <IDENTITY>`
- `zeroclaw channel add <type> <json>`
- `zeroclaw channel remove <name>`

Runtime in-chat commands (Telegram/Discord while channel server is running):

- `/models`
- `/models <provider>`
- `/model`
- `/model <model-id>`

Channel runtime also watches `config.toml` and hot-applies updates to:
- `default_provider`
- `default_model`
- `default_temperature`
- `api_key` / `api_url` (for the default provider)
- `reliability.*` provider retry settings

`add/remove` currently route you back to managed setup/manual config paths (not full declarative mutators yet).

### `integrations`

- `zeroclaw integrations info <name>`

### `skills`

- `zeroclaw skills list`
- `zeroclaw skills audit <source_or_name>`
- `zeroclaw skills install <source>`
- `zeroclaw skills remove <name>`

`<source>` accepts git remotes (`https://...`, `http://...`, `ssh://...`, and `git@host:owner/repo.git`) or a local filesystem path.

`skills install` always runs a built-in static security audit before the skill is accepted. The audit blocks:
- symlinks inside the skill package
- script-like files (`.sh`, `.bash`, `.zsh`, `.ps1`, `.bat`, `.cmd`)
- high-risk command snippets (for example pipe-to-shell payloads)
- markdown links that escape the skill root, point to remote markdown, or target script files

Use `skills audit` to manually validate a candidate skill directory (or an installed skill by name) before sharing it.

Skill manifests (`SKILL.toml`) support `prompts` and `[[tools]]`; both are injected into the agent system prompt at runtime, so the model can follow skill instructions without manually reading skill files.

### `migrate`

- `zeroclaw migrate openclaw [--source <path>] [--dry-run]`

### `config`

- `zeroclaw config schema`

`config schema` prints a JSON Schema (draft 2020-12) for the full `config.toml` contract to stdout.

### `completions`

- `zeroclaw completions bash`
- `zeroclaw completions fish`
- `zeroclaw completions zsh`
- `zeroclaw completions powershell`
- `zeroclaw completions elvish`

`completions` is stdout-only by design so scripts can be sourced directly without log/warning contamination.

### `hardware`

- `zeroclaw hardware discover`
- `zeroclaw hardware introspect <path>`
- `zeroclaw hardware info [--chip <chip_name>]`

### `peripheral`

- `zeroclaw peripheral list`
- `zeroclaw peripheral add <board> <path>`
- `zeroclaw peripheral flash [--port <serial_port>]`
- `zeroclaw peripheral setup-uno-q [--host <ip_or_host>]`
- `zeroclaw peripheral flash-nucleo`

## Validation Tip

To verify docs against your current binary quickly:

```bash
zeroclaw --help
zeroclaw <command> --help
```
