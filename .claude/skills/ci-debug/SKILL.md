# CI Debug Skill
1. Read the failing CI log carefully
2. Identify ALL failing tests and their error messages
3. Check runtime state before investigating code (is the service running? correct remote?)
4. Fix ALL required fields/params in one pass - review full struct signatures
5. Push to 'upstream' remote
6. Run `cargo check` and `cargo clippy` before pushing
