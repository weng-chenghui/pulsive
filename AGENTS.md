# Agent Guidelines

This document outlines the guidelines for AI agents working on this codebase.

---

## 1. Git Commit Signing

**All commits should be signed locally.** Do not use sandbox mode when making commits.

- Always attempt to sign commits with GPG/SSH signing
- If commit signing fails, **stop making commits immediately**
- Instead, provide the user with the exact commands to:
  1. Stage the changes: `git add <files>`
  2. Commit with the proper message: `git commit -S -m "<commit message>"`
  3. Push to remote: `git push origin <branch>`

Example fallback output:
```bash
# Commit signing failed. Please run these commands manually:
git add <files>
git commit -S -m "feat(core): add new entity system"
git push origin feature/entity-system
```

---

## 2. Local Testing Before Push

**All tests must pass locally before pushing to remote.**

### Pre-Push Verification

Run the full test suite using the Makefile before pushing:

```bash
make check    # Runs: fmt, clippy, build, test, docs
# or
make pre-push # Alias for check
```

### Extending Test Coverage

When adding new components/crates, ensure they are covered by:

1. **Makefile**: Update the workspace exclusion list if needed
   - Current exclusion: `WORKSPACE_EXCLUDE := --exclude pulsive-godot`
   - Add new crates to test targets if they require special handling

2. **Git Hook**: The `scripts/pre-push` hook mirrors CI checks
   - Install with: `make install-hooks`
   - Ensure new components are included in the hook's test scope

3. **CI Parity**: Local checks should match CI behavior
   - Same `RUSTFLAGS` and `RUSTDOCFLAGS` settings
   - Same clippy and formatting rules

---

## 3. Temporary Scripts Management

**Temporary scripts should be organized in `.scripts/` with proper documentation.**

### Directory Structure

Create a new subdirectory for each temporary script with a datetime prefix:

```
.scripts/
├── 2025-12-06_merge-prs/
│   ├── README.md
│   ├── merge-prs.sh
│   └── merge-log.txt
├── 2025-12-07_data-migration/
│   ├── README.md
│   ├── migrate.py
│   └── rollback.py
└── ...
```

### Naming Convention

- Directory format: `YYYY-MM-DD_descriptive-name/`
- Use lowercase with hyphens for the descriptive name
- Include timestamp when multiple scripts are created on the same day: `YYYY-MM-DD-HHMM_name/`

### Required README.md

Each script subdirectory **must** include a `README.md` with:

1. **Purpose**: Brief description of why this script is needed
2. **Context**: What problem it solves or what task it automates
3. **Usage Examples**: How to run the script with example commands
4. **Prerequisites**: Any dependencies or setup required
5. **Cleanup**: Whether the script/directory can be deleted after use

Example `README.md`:

```markdown
# Merge PRs Script

## Purpose
Automates sequential merging of multiple PRs while waiting for CI to pass between each merge.

## Context
Created to batch-merge a series of related PRs after a major refactoring.

## Usage
```bash
# Edit the PRS array in the script to list PR numbers
./merge-prs.sh
```

## Prerequisites
- `gh` CLI installed and authenticated
- `jq` for JSON parsing

## Cleanup
Safe to delete after all PRs are merged.
```

---

## 4. RFC for Major Changes

**Create an RFC (Request for Comments) as a GitHub Issue when planning big features or significant changes.**

### When to Create an RFC

- New major features that affect multiple crates
- Breaking changes to public APIs
- Architectural changes
- Changes that require coordination across team members
- Features with multiple implementation approaches

### RFC Issue Template

Suggest the following structure for RFC issues:

```markdown
## Summary
Brief one-paragraph description of the proposed change.

## Motivation
Why is this change needed? What problem does it solve?

## Detailed Design
Technical details of the proposed implementation.

## Alternatives Considered
Other approaches that were evaluated and why they were rejected.

## Impact
- Breaking changes?
- Migration path?
- Affected crates/components?

## Open Questions
Unresolved design decisions that need discussion.
```

### Labels

Suggest using labels like:
- `RFC`
- `discussion`
- `breaking-change` (if applicable)

---

## 5. Maintain General Design Principles

**New features to general/shared code must maintain generic, interface-based design.**

### Design Guidelines

1. **Use Traits for Abstractions**
   - Define behavior through traits, not concrete types
   - Allow consumers to provide their own implementations
   - Keep trait definitions focused and minimal

2. **Prefer Composition Over Inheritance**
   - Use trait bounds and generics
   - Compose functionality through multiple traits

3. **Interface-Implementation Separation**
   - Core traits in `pulsive-core`
   - Concrete implementations in specific crates
   - Keep implementation details private

### Examples from Codebase

The codebase already follows these patterns:

- `pulsive-core`: Defines core traits and abstractions
- `pulsive-db`: Implements storage-specific logic
- `pulsive-journal`: Implements journaling behavior

### Code Review Checklist

When reviewing new code additions:

- [ ] Does it use traits where behavior needs to be abstract?
- [ ] Are concrete types hidden behind trait interfaces where appropriate?
- [ ] Can different implementations be swapped without changing consumers?
- [ ] Are generics used appropriately (not over-engineered)?
- [ ] Does the design follow existing patterns in the codebase?

---

## 6. Maximize Pulsive Architecture Utilization

**Every new feature, big change, or demo case must fully leverage the pulsive architecture.**

### Requirement

When implementing new functionality, ensure the code:

1. **Uses core pulsive abstractions** - Entities, Commands, Effects, Messages, State History, etc.
2. **Follows the event-driven model** - Proper use of the journal system and replay capabilities
3. **Leverages deterministic simulation** - RNG, time management, and rollback features where applicable
4. **Integrates with the runtime** - Not bypassing the pulsive runtime with ad-hoc solutions

### Anti-Pattern: Superficial Framework Usage

❌ **Do NOT** create demos or features that:
- Only use 1-2 basic types from `pulsive-core`
- Implement core logic outside the pulsive paradigm
- Claim to be a "pulsive example" while mostly using vanilla Rust patterns
- Bypass the command/effect system with direct state mutations

### Validation Checklist

Before considering a new feature or demo complete:

- [ ] Does it use the Entity system appropriately?
- [ ] Are state changes driven through Commands and Effects?
- [ ] Is the state replayable/auditable via the journal?
- [ ] Could this demo showcase pulsive's unique capabilities?
- [ ] Would removing pulsive significantly change the architecture?

### Example Questions to Ask

- "If I removed pulsive from this code, would it still work mostly the same?" → If yes, reconsider the design
- "Does this demo show why someone would choose pulsive over vanilla Rust?" → If no, add more framework integration
- "Can I replay/audit the state changes in this feature?" → If no, integrate the journal system

---

## 7. Feature Branches and Pull Requests

**Work that closes GitHub issues should be done in feature branches with pull requests.**

### Workflow

1. **Create a feature branch** from `main`:
   ```bash
   git checkout -b feat/<feature-name>
   # or for fixes:
   git checkout -b fix/<issue-description>
   ```

2. **Implement the changes** on the feature branch

3. **Push the branch and immediately create a PR using `gh`**:
   ```bash
   git push origin feat/<feature-name>
   gh pr create --title "feat: <description>" --body "Closes #<issue-number>"
   ```

### PR Creation Requirement

**Always create a PR immediately after pushing a feature branch.** Use the GitHub CLI (`gh`) to create PRs programmatically:

```bash
# Basic PR creation
gh pr create --title "<type>: <description>" --body "Closes #<issue-number>"

# With more details
gh pr create \
  --title "feat: <description>" \
  --body "## Summary
<brief description>

## Changes
- Change 1
- Change 2

Closes #<issue-number>"
```

Do not leave feature branches without associated PRs.

### Branch Naming Convention

- `feat/<name>` - New features
- `fix/<name>` - Bug fixes
- `refactor/<name>` - Code refactoring
- `docs/<name>` - Documentation changes
- `chore/<name>` - Maintenance tasks

### PR Requirements

- Link the related issue using `Closes #<number>` in the PR description
- Ensure all tests pass before requesting review
- Keep PRs focused on a single issue/feature when possible

### Exceptions

Direct commits to `main` are acceptable for:
- Typo fixes in documentation
- Emergency hotfixes (with immediate follow-up PR for review)
- Automated sync operations (e.g., `.cursorrules` sync)

---

## 8. AGENTS.md and .cursorrules Sync

**`AGENTS.md` is the source of truth. `.cursorrules` is automatically synced as a mirror.**

### Architecture

```
AGENTS.md (source of truth)
    │
    ├── Manual edits happen here
    │
    └── Auto-syncs to → .cursorrules (mirror for Cursor IDE)
```

### Sync Mechanism

- **Pre-commit hook**: Automatically syncs `AGENTS.md` → `.cursorrules` on every commit
- **Manual sync**: Run `make sync-agents` to manually sync

### Installation

```bash
make install-hooks  # Installs both pre-commit and pre-push hooks
```

### Rules

1. **Always edit `AGENTS.md`**, never `.cursorrules` directly
2. The pre-commit hook will automatically update `.cursorrules`
3. If `.cursorrules` is out of sync, run `make sync-agents`

---

## Quick Reference

| Task | Action |
|------|--------|
| Commit changes | Sign with `-S` flag, fallback to manual commands |
| Before pushing | Run `make check` or `make pre-push` |
| Closing an issue | Feature branch + `gh pr create` (not direct to main) |
| Temporary script | Create `YYYY-MM-DD_name/` in `.scripts/` with README |
| Big feature | Create RFC issue on GitHub first |
| New shared code | Use traits and interface-based design |
| New feature/demo | Validate full pulsive architecture utilization |
| Sync agent rules | Run `make sync-agents` (auto on commit) |
