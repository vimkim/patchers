# Patchers: a TUI Diff Hunk Selector

> I'm tired of scrolling long diff patch files.

A fast, minimal **terminal UI for interactively selecting hunks from a unified diff** and writing out a **filtered patch**.  
This tool allows you to curate large patches into smaller, reviewable, or cherry-pickable subsets with immediate visual feedback.

Built with **Rust**, powered by **ratatui** and **crossterm**.

<img width="2534" height="1307" alt="image" src="https://github.com/user-attachments/assets/d122222b-3b32-4abf-977c-9d322e7ed681" />


---

## Features

- Parse standard **unified diffs** (`git diff`, `git format-patch`, etc.)
- Interactive **TUI hunk browser**
- **Live preview** with colored additions/removals
- Toggle hunks on/off and **auto-save** the filtered patch
- Preserves original **file headers and metadata**
- Fully keyboard-driven
- Lightweight and dependency-minimal

---

## Demo Workflow

1. Load a patch file
2. Navigate between hunks
3. Toggle only the changes you want
4. Instantly produce a clean filtered patch for:
   - Partial code reviews
   - Stacked diffs
   - Backports
   - Cherry-picking across branches

---

## Installation

### Install with Git

```bash
cargo install --git https://github.com/vimkim/patchers.git patchers
```

### Install with Git Clone

```bash
git clone https://github.com/yourname/patchers.git
cd patchers
cargo install --path .
```

### Build from Source

```bash
git clone https://github.com/yourname/patchers.git
cd patchers
cargo build --release
````

Binary will be available at:

```bash
target/release/patchers
```

---

## Usage

```bash
patchers <INPUT_PATCH> --output <OUTPUT_PATCH>
```

### Example

```bash
git diff > diff.patch
patchers diff.patch -o filtered.patch
```

While running:

| Key             | Action             |
| --------------- | ------------------ |
| `↑ / k`         | Move up            |
| `↓ / j`         | Move down          |
| `Space / Enter` | Toggle hunk & save |
| `q`             | Quit               |

Each toggle **immediately writes the output file**, so your filtered patch is always up to date.

---

## Input Format

* Expects a **standard unified diff**
* Fully compatible with:

  * `git diff`
  * `git show`
  * `git format-patch`
* Handles:

  * Multi-file patches
  * Arbitrary metadata sections
  * `\ No newline at end of file`

---

## Output Format

* Writes a **valid unified diff**
* Preserves:

  * `diff --git` headers
  * `index`, `---`, `+++` lines
* Includes **only selected hunks**
* Safe to apply with:

```bash
git apply filtered.patch
```

---

## UI Overview

* **Left panel**: Hunk list with file labels and previews
* **Right panel**: Colored diff preview
* **Bottom panel**: Status & key bindings
* `[x]` indicates selected hunks
* `[ ]` indicates unselected hunks

---

## Error Handling

* Graceful parsing of malformed diffs
* Live status feedback on save errors
* Clean terminal teardown on panic or exit

---

## Dependencies

* [`anyhow`](https://crates.io/crates/anyhow)
* [`clap`](https://crates.io/crates/clap)
* [`crossterm`](https://crates.io/crates/crossterm)
* [`ratatui`](https://crates.io/crates/ratatui)

---

## Use Cases

* Preparing **minimal review patches**
* Splitting large diffs into logical commits
* Creating **safe hotfix patches**
* Extracting specific features from large changes
* Teaching and demonstration of patch mechanics

---

## Development

```bash
cargo run -- diff.patch -o filtered.patch
```

Formatting:

```bash
cargo fmt
```

Linting:

```bash
cargo clippy
```

---

## License

MIT License.
You are free to use, modify, and distribute this tool.

---

## Contributing

Pull requests are welcome.
Please follow standard Rust formatting and keep commits focused.

Suggested areas for improvement:

* File-level toggling
* Search / filter hunks
* Unified scroll for large hunks
* Mouse support
* Git integration mode

---

## Acknowledgments

* `ratatui` for the TUI framework
* `crossterm` for cross-platform terminal control
* Git’s unified diff format

