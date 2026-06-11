# Instructions for Migration Agents

Before writing or editing SQL migrations in this directory, read `backend/migrations/README.md`.

Keep migrations as SQLx SQL files.
Do not replace schema migrations with Rust code.
Before MVP, a migration-history squash must explicitly document that existing databases need to be recreated.
After MVP or once real persistent production data exists, treat migrations as append-only and add the next numbered SQL file instead of editing an applied file.
