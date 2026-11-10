# Philosophy

How this project is built. It combines the Unix philosophy with John
Ousterhout's *A Philosophy of Software Design* (APOSD). The rules apply to
humans and agents alike. The quality gates enforce what they can. Review
enforces the rest.

## The one rule

**Complexity is the enemy.** Complexity is anything that makes the system
hard to understand or change. It shows up as change amplification (one
change touches many places), cognitive load (what you must know to make a
change), and unknown unknowns (what you cannot tell you need to know). Every
rule below is a way to keep it down.

## Design

1. **Deep modules.** A module's interface should be much simpler than its
   implementation. Few entry points, each doing a lot. A shallow module, whose
   interface is about as complex as its body, costs more than it gives. Do not
   split code just to make files or functions short.
2. **Hide information.** Each module owns one design decision (a format, a
   protocol, a storage layout) and no other module knows it. If two modules
   must change together, they leak one decision. Merge them or move the
   decision.
3. **Pull complexity down.** When something must be complicated, the module
   absorbs it, not its callers. A simple interface with a hard implementation
   beats the reverse.
4. **General-purpose core, special-purpose edges.** The core's interfaces
   are a little more general than today's callers need. Special cases
   belong in the front ends.
5. **Define errors out of existence.** Design the API so the error cannot
   happen (deleting a missing key succeeds, an empty range is a valid range)
   before you add a code path for it. Each error that remains gets a stable
   code and a hint that tells the caller what to do.
6. **Design it twice.** For any interface that matters, sketch two real
   alternatives before picking one. The first idea is rarely the best.
7. **Strategic, not tactical.** Working code is not enough. Spend a little
   on every change to leave the design better. "Just get it working" is how
   complexity piles up, and it piles up one reasonable shortcut at a time.

## Composition (Unix)

1. **Do one thing well.** A tool or module has a purpose you can say in one
   sentence. When it grows a second one, split it into a separate tool, not
   a flag.
2. **One core, thin front ends.** Domain logic lives in a core library with
   no terminal, GUI or network concerns. The CLI, GUI and service only
   translate input into calls and results into output, so each capability
   has one implementation.
3. **Text streams are the interface.** Output is structured enough to pipe
   and parse: plain text by default, `--json` for machines, nothing extra on
   stdout. Data formats are API: once released, change them only in ways
   that do not break readers.
4. **Silence is golden.** A command that succeeds says only what was asked.
   Progress and chatter go to stderr and `--quiet` turns them off.
5. **Fail loudly and early.** Bad input, a missing config or an unknown
   option stops the program with a clear message and a non-zero, documented
   exit code. Never fall back silently.
6. **Build to compose.** Expect your output to become another program's
   input. Typed exit codes, stable JSON and no interactive prompts in
   non-TTY mode make that possible.

## Code

- **Names carry the design.** A precise name is the cheapest documentation.
  If it is hard to name, the design is unclear.
- **Comments say what the code cannot.** State intent, constraints, units
  and invariants: the why, not the what. Interface comments describe the
  abstraction, not the implementation. Narration of the obvious is noise.
- **Consistency over cleverness.** Do the same thing the same way everywhere.
  Follow the nearest existing code.
- **Delete unused features.** Code nobody uses still costs maintenance and
  understanding. Delete it. It can come back when someone needs it.
- **Seams are paid for.** Add an abstraction (trait, interface, registry)
  when the second implementation arrives, not on speculation.

## Verification

- **Ratchets only tighten.** Coverage floors rise, and size, debt and
  duplication budgets fall. Loosening one is a deliberate, reviewed commit
  with a reason. Nothing is grandfathered at birth.
- **Tests verify behavior.** Test through public interfaces with real
  inputs, including malformed and boundary cases. A test that asserts a mock
  was called documents the implementation and verifies nothing.
- **One gate command.** `verify` runs every gate, locally, in the pre-commit
  hook and in CI, which run the same thing. If a gate is too slow for the
  hook, make it faster. Do not skip it.
- **Evidence or it didn't happen.** Claims cite commits, paths and command
  output. "All gates green" is necessary, not sufficient: refactors also
  check paths the tests do not import (CI, docs, config).

## Process

- **Merges are truthful.** A commit or PR title describes its diff. A red
  gate is fixed, never explained away.
- **Releases mean something.** A release has at least one change a user
  can observe. The CHANGELOG is written for users.
- **Small, reviewable steps.** One concern per commit. Conventional commit
  messages (`feat(scope): ...`) that cite the tracker id.
