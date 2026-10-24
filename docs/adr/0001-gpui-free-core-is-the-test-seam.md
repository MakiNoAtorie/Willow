# The GPUI-free core is the test seam

The document model, every editing command (create, link, insert-and-push, delete, recolor, zone changes), the undo history and the text format live in a core crate with no GPUI dependency. Tests drive that command API and assert on the model and on saved text; the GPUI layer only turns input into commands and draws the model. We chose this over testing through GPUI's harness because GPUI is unstable and pinned to a git revision, UI tests are slow, and Phase 0 may still swap the UI stack for Linebender: the core survives that swap untouched.
