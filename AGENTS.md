# RitKraken repository rules

- Every source code file must be at most 1,000 lines. Generated files (such as `Cargo.lock`) and non-code assets are exempt.
- Split source files by responsibility before they reach the limit. Keep Git operations in `crates/git` and desktop features in `crates/desktop`.
- Use the Git installed on the system. Git 2.37 is the compatibility baseline for required commands, arguments, and output formats.
- Before implementing a behavior, check for an existing implementation and reuse it. When multiple callers need the same behavior, keep one implementation in the module that owns it and have all callers use it.
- Run `python tools/check_project.py` before handing off changes. A line-count violation must be fixed before handoff.
- Reuse the fixed platform build directory for builds, tests, and verification: `target/windows` on Windows and `target/macos` on macOS. Use the root build scripts for deliverables and the same Rust target and target directory for Cargo checks. Never create build or executable-copy directories named after a task, run, commit, or UI change.
- If Windows locks the output executable, report the executable that needs to close. Do not bypass the lock by changing output directories or copying the application to another location. Do not terminate a user's running application without authorization.
- Reuse `.build/qa` and `.build/test-tmp` for temporary verification. Remove disposable application copies, project snapshots, and test repositories after verification; retain only small reports when needed. Preserve the shared Cargo and Rust toolchain caches.
