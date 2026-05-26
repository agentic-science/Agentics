# Database Migrations

These SQLx migrations are the source of truth for the PostgreSQL schema. Keep
schema changes in SQL files; Rust commands such as `agentics-migrate` only run
the SQLx migrator.

Agentics is still pre-MVP, so this directory may contain a squashed baseline
when the team intentionally resets migration history. Any database that has
applied a previous migration history must be dropped and recreated after such a
reset because SQLx records migration versions and checksums in
`_sqlx_migrations`.

After MVP, or once real persistent production data exists, migrations are
append-only. Do not edit applied migration files; add the next numbered SQL
file instead. This baseline is split by domain for readability, but all files
are required for a valid Agentics database.
