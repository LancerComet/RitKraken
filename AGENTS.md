# RitKraken repository rules

- Every project file must be at most 1,000 lines, including `Cargo.lock`.
- Split source files by responsibility before they reach the limit. Keep Git operations in `crates/git` and desktop features in `crates/desktop`.
- Before implementing a behavior, check for an existing implementation and reuse it. When multiple callers need the same behavior, keep one implementation in the module that owns it and have all callers use it.
- Run `python tools/check_project.py` before handing off changes. A line-count violation must be fixed before handoff.
