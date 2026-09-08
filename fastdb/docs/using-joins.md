# USING join implementation work

Collection SELECT lowering still rejects USING and NATURAL joins. This note records the pinned native oracle and required lowering work; it does not claim collection support.

`tests/tests/using.rs` verifies ordinary native tables `a(k,left_value)` and `b(k,right_value)`, with matching key 1 and unmatched keys 2/3:

- INNER and LEFT JOIN USING(k) expand unqualified `*` to `k,left_value,right_value`.
- RIGHT JOIN USING(k) expands it to `left_value,k,right_value`: the retained key belongs to the right source.
- Qualified stars retain both key positions. Qualified key references retain each source's NULL extension.
- Unqualified `k` resolves to the retained side, including the unmatched right row.
- The tested FULL JOIN USING(k) form rejects with the pinned equality-condition error. Do not silently implement a different native behavior.

Implementation needs a shared merged-column representation before projection/star expansion, field resolution and join-predicate lowering. Replacing USING with ON alone loses unqualified-name resolution and star suppression. Preserve original equality operand order for affinity/collation, source-specific qualified access, left/right NULL extension and post-join filtering. Chained joins need the previously merged left input, not an arbitrary physical source.

Collection stars still represent whole documents under the existing result contract. Do not turn them into variable-schema field expansion while adding join keys. Closed derived/native stars need the pinned suppression/order rules. Explicit USING names must resolve consistently without discovering optional collection fields by executing queries.

Before enabling the syntax, extend the oracle for multiple keys, chained joins, case/quoted names, duplicate outputs, missing keys, NULLs, native affinity/collation and typed record/binary keys. Cover GROUP/ORDER aliases, execute/profile/EXPLAIN, INSERT SELECT constraints and rollback. NATURAL joins require a separate known-column intersection policy and must not be enabled incidentally.
