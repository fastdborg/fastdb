# Clean-Room Policy

FastDB implements a SurrealQL-compatible subset from public documentation
and independently observed public behavior. This document defines what is
allowed, what is forbidden, how to record provenance, the reviewer
checklist, and the trademark/compatibility disclaimer. It is normative
for all FastDB compatibility work.

## Behavioral reference

- SurrealDB `v3.1.5` is the pinned behavioral reference.
- Compatibility is a documented **subset**, not full compatibility.

## Allowed sources

1. **Public SurrealQL documentation** (e.g. the reference docs linked in
   `revised_plan.md` section 15). Cite the URL and access date.
2. **Black-box observations** against an unmodified SurrealDB `v3.1.5`
   binary: run queries, record exact input, exact output, binary version,
   OS/date, and the public source of the binary. Store these notes under
   `docs/compat-research/`.
3. **Independently written** FastDB parsers, ASTs, tests, fixtures,
   fuzz corpora, and expected outputs.

## Forbidden actions

Do **not** copy, translate, adapt, or vendor any of the following from
SurrealDB:

- source code,
- test files or fixtures,
- expected-output files,
- fuzz corpora,
- implementation details or internal data structures.

Do not read SurrealDB source to guide FastDB implementation. If you have
read it, recuse yourself from the corresponding FastDB code and document
the recusal.

SurrealDB is an independent project with its own licenses and trademarks.
FastDB's project owner has approved the current product name and the precise
compatibility wording below; this does not grant rights in third-party marks.

## Provenance-note format

Every compatibility decision must trace to an allowed source. Record, at
minimum:

```markdown
### <feature id>: <short description>
- Source kind: public-doc | black-box | fastdb-choice
- Reference: <URL or binary version + OS/date>
- Input: <exact query/input>
- Observed/expected output: <exact output>
- FastDB behavior: <implemented | rejected as unsupported | differs because>
- Date: YYYY-MM-DD
- Author: <name/handle>
```

Store these under `docs/compat-research/`. A reviewer must be able to
tell which behavior came from public documentation, which came from a
black-box observation, and which is a FastDB-specific choice.

## Reviewer checklist

Before marking a compatibility row `supported`:

- [ ] An allowed-source provenance note exists and is cited from `COMPAT.md`.
- [ ] No SurrealDB source/test/fixture was copied, translated, or adapted.
- [ ] Tests are independently authored.
- [ ] Accepted forms are executed; rejected forms return an explicit
      `UnsupportedSyntax` (or parse) error and are never silently ignored.
- [ ] The `COMPAT.md` row status, test ID, and provenance reference agree.

## Trademark and compatibility disclaimer

"SurrealQL-compatible subset" describes a documented behavioral target.
It does **not** imply:

- sponsorship or affiliation with SurrealDB,
- certification,
- complete or guaranteed compatibility.

Use "SurrealQL-compatible subset" (not "SurrealDB-compatible") where
precision matters. Do not use SurrealDB names or marks in a way that implies
sponsorship, affiliation, certification, or ownership.
