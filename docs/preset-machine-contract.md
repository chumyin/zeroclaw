# Preset Machine JSON Contract

Machine-readable runtime contract for preset orchestration and composition APIs.

Last updated: **February 22, 2026**.

## Purpose

This document is for:

- agent/runtime integrators that execute `zeroclaw preset ... --json`
- UI builders that need stable fields for confirmations and risk prompts
- CI automation that validates preset payload quality before import/share

For user workflow and contribution guidance, see:

- [presets-guide.md](presets-guide.md)
- [preset-recommendation-matrix.md](preset-recommendation-matrix.md)
- [../presets/community/README.md](../presets/community/README.md)

## Scope

The contract in this file covers these report types:

- `preset.intent_orchestration`
- `preset.apply_dry_run`
- `preset.import_dry_run`
- `preset.export`
- `preset.validation`

Command to report mapping:

| Command | Report type | Writes workspace | Writes export file |
|---|---|---|---|
| `zeroclaw preset intent "<text>" --json` | `preset.intent_orchestration` | No | No |
| `zeroclaw preset apply ... --dry-run --json` | `preset.apply_dry_run` | No | No |
| `zeroclaw preset import <path> --dry-run --json` | `preset.import_dry_run` | No | No |
| `zeroclaw preset export <path> --json` | `preset.export` | No | Yes |
| `zeroclaw preset validate <path...> --json` | `preset.validation` | No | No |

## Compatibility Rules

### 1) Dispatch by `report_type`

Consumers should branch on `report_type` first, then parse fields for that type.

### 2) Version gate with `schema_version`

- Current contract version: `1`
- If `schema_version` is greater than the max supported by your client, fail safely and prompt for client upgrade.

### 3) Additive evolution only

- New fields may be added in `schema_version = 1`.
- Existing fields should not be renamed or removed in the same schema version.
- Clients must tolerate unknown fields.

### 4) JSON purity

For commands listed in scope, `--json` mode prints JSON to stdout only.
Human-readable notices (if any) are printed to stderr so stdout remains machine-parseable.

## Shared Semantics

### Consent fields

When a report (or generated next command) requires explicit user confirmation, it includes:

- boolean gate (for example `apply_requires_explicit_consent` or `requires_explicit_consent`)
- `*_consent_reasons`: stable symbolic codes
- `*_consent_reason_keys`: i18n-safe keys for localized UI copy

Current reason code mapping:

| reason code | i18n key | meaning |
|---|---|---|
| `risky_pack` | `consent.reason.risky_pack` | selected composition includes risk-gated packs |
| `rebuild` | `consent.reason.rebuild` | action would trigger compile/rebuild |
| `security_non_strict` | `consent.reason.security_non_strict` | action lowers security strictness |

### Rebuild preview

`rebuild_preview` appears when a preview includes rebuild context and contains:

- `command`: full rebuild command
- `working_directory`: execution directory
- `would_execute`: always `false` in dry-run reports

## Report Contracts

### `preset.intent_orchestration`

Command:

```bash
zeroclaw preset intent "need unattended browser automation" --json
```

Required fields:

- `schema_version: number`
- `report_type: "preset.intent_orchestration"`
- `intent: string`
- `capability_sources: string[]`
- `plan: object` (intent parse and ranking result)
- `planned_selection: object`
- `risky_packs: string[]`
- `security_recommendation: object`
- `security_apply_command: string`
- `next_commands: object[]`

`next_commands[]` contract:

- `id: string` (stable operation identifier, for example `preset.apply`, `security.profile.set`)
- `description: string`
- `command: string` (ready-to-run CLI)
- `requires_explicit_consent: boolean`
- `consent_reasons?: string[]`
- `consent_reason_keys?: string[]`

Example (trimmed):

```json
{
  "schema_version": 1,
  "report_type": "preset.intent_orchestration",
  "intent": "need unattended browser automation",
  "next_commands": [
    {
      "id": "preset.apply",
      "requires_explicit_consent": true,
      "consent_reasons": ["risky_pack"],
      "consent_reason_keys": ["consent.reason.risky_pack"],
      "command": "zeroclaw preset intent 'need unattended browser automation' --apply --yes-risky"
    }
  ]
}
```

### `preset.apply_dry_run`

Command:

```bash
zeroclaw preset apply --preset automation --pack rag-pdf --dry-run --json
```

Guardrail:

- `--json` is valid only with `--dry-run` for `preset apply`.

Required fields:

- `schema_version: number`
- `report_type: "preset.apply_dry_run"`
- `previous_selection: object | null`
- `planned_selection: object`
- `selection_diff: object`
- `risky_packs: string[]`
- `apply_requires_explicit_consent: boolean`
- `rebuild_requested: boolean`
- `workspace_written: boolean` (always `false` in this report type)

Optional fields:

- `apply_consent_reasons: string[]`
- `apply_consent_reason_keys: string[]`
- `warnings: string[]`
- `rebuild_preview: object`

### `preset.import_dry_run`

Command:

```bash
zeroclaw preset import ./team.preset.json --mode merge --dry-run --json
```

Guardrail:

- `--json` is valid only with `--dry-run` for `preset import`.

Required fields:

- `schema_version: number`
- `report_type: "preset.import_dry_run"`
- `import_mode: "overwrite" | "merge" | "fill"`
- `source_path: string`
- `previous_selection: object | null`
- `planned_selection: object`
- `selection_diff: object`
- `risky_packs: string[]`
- `apply_requires_explicit_consent: boolean`
- `rebuild_requested: boolean`
- `workspace_written: boolean` (always `false` in this report type)

Optional fields:

- `apply_consent_reasons: string[]`
- `apply_consent_reason_keys: string[]`
- `warnings: string[]`
- `rebuild_preview: object`

### `preset.export`

Command:

```bash
zeroclaw preset export ./team-export.preset.json --json
```

Write semantics:

- This report is not dry-run.
- Export payload is written to disk.
- JSON report describes what was written.

Required fields:

- `schema_version: number`
- `report_type: "preset.export"`
- `source_kind: "official_preset" | "workspace_selection" | "default_selection"`
- `selection: object`
- `target_path: string`
- `bytes_written: number`
- `payload_sha256: string` (hex lowercase)
- `write_performed: boolean` (currently `true`)

Optional fields:

- `requested_preset: string` (present when `--preset <ID>` is used)

Example (trimmed):

```json
{
  "schema_version": 1,
  "report_type": "preset.export",
  "source_kind": "official_preset",
  "requested_preset": "automation",
  "target_path": "/tmp/exported.preset.json",
  "bytes_written": 324,
  "payload_sha256": "e3b0c44298fc1c149afbf4c8996fb924...",
  "write_performed": true
}
```

### `preset.validation`

Command:

```bash
zeroclaw preset validate ./presets/community --json
```

Required fields:

- `schema_version: number`
- `report_type: "preset.validation"`
- `files_checked: number`
- `files_failed: number`
- `allow_unknown_packs: boolean`
- `results: object[]`

`results[]` fields:

- `path: string`
- `format: string` (`json` or `toml`)
- `ok: boolean`
- `errors: string[]`

## Integration Checklist

1. Execute command with `--json` and parse stdout as JSON.
2. Validate `schema_version` and `report_type`.
3. If consent gate is true, show reasons (`*_consent_reasons`) and localized copy (`*_consent_reason_keys`) before continuing.
4. For `preset.export`, verify `payload_sha256` and `bytes_written` if integrity checks are required.
5. Store unknown fields for forward compatibility, do not treat them as errors.

## Non-goals

- This document does not define provider auth flows.
- This document does not redefine preset payload schema authoring rules (see `presets-guide.md` and `presets/community/README.md`).
