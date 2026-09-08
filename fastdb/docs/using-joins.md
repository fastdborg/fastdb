# USING join implementation work

Initial collection USING lowering now supports INNER/LEFT/leading-RIGHT joins, chained merged keys, qualified key references and closed-source star suppression. The implementation builds comparison predicates in normalized operand order and keeps merged-key bindings separate from source columns. Collection stars remain whole documents. NATURAL and FULL USING remain rejected; this is not complete join release qualification.

`tests/tests/using.rs` verifies ordinary native tables `a(k,left_value)` and `b(k,right_value)`, with matching key 1 and unmatched keys 2/3:

- INNER and LEFT JOIN USING(k) expand unqualified `*` to `k,left_value,right_value`.
- RIGHT JOIN USING(k) expands it to `left_value,k,right_value`: the retained key belongs to the right source.
- Qualified stars retain both key positions. Qualified key references retain each source's NULL extension.
- Unqualified `k` resolves to the retained side, including the unmatched right row.
- The tested FULL JOIN USING(k) form rejects with the pinned equality-condition error. Do not silently implement a different native behavior.

The implementation now builds merged-column metadata before projection/star expansion, field resolution and join-predicate lowering. Replacing USING with ON alone loses unqualified-name resolution and star suppression. Preserve the pinned planner's normalized equality operand order for affinity/collation, source-specific qualified access, left/right NULL extension and post-join filtering. Chained joins need the previously merged left input, not an arbitrary physical source.

Collection stars still represent whole documents under the existing result contract. Do not turn them into variable-schema field expansion while adding join keys. Closed derived/native stars need the pinned suppression/order rules. Explicit USING names must resolve consistently without discovering optional collection fields by executing queries.

Remaining qualification must extend the implementation tests for multiple keys, chained joins, case/quoted names, duplicate outputs, missing keys, NULLs, native affinity/collation and typed record/binary keys. Cover GROUP/ORDER aliases, execute/profile/EXPLAIN, INSERT SELECT constraints and rollback. NATURAL joins require a separate known-column intersection policy and must not be enabled incidentally.


The multiple-key/chained oracle now also verifies that USING(t,k) does not reorder the retained table columns. A LEFT JOIN on (k,t) followed by JOIN c USING(k) retains the left key when the earlier right row is unmatched. The pinned `a RIGHT JOIN b USING(k,t) JOIN c USING(k)` star expands to `c,a,k,t,b` for the fixture, reflecting engine join reordering. Do not assume written FROM order when implementing RIGHT-join stars; inspect the pinned planner's join normalization and preserve qualified-star behavior separately.

Mixed ON-query unqualified star ordering now mirrors the pinned leading-RIGHT swap/reverse behavior, with differential read/write tests. Merged USING keys and collection USING syntax now have initial support, as described above.


The collation oracle verifies that RIGHT JOIN USING compares the written right-side key first after normalization. With left NOCASE 'A' and right BINARY 'a', INNER/LEFT USING match while RIGHT USING does not; explicit ON a.k=b.k still matches. Reversing the written sources reverses these USING comparison outcomes. A textual USING-to-ON rewrite with unchanged written operand order is therefore incorrect.


Initial implementation verification covers 24 closed-source read shapes (inner/left/right, optional following USING join, unqualified/qualified stars, merged/qualified keys and arithmetic), direct collection key lookup, normal/profiled results, normalized NOCASE/BINARY comparison order, typed record-key writes with uniqueness failure/rollback/reuse, and both Node clients. Aliases/grouping/window scopes, correlated references, duplicate-key columns, missing/native virtual columns and complete resource/evaluation qualification remain open.

A 27-case multi-key implementation matrix now matches native output names and rows for reordered/case-varied/quoted key lists, INNER/LEFT/RIGHT joins and post-join NULL/merged-key filtering through execute/profile.

Closed-source grouping/alias qualification now includes eight normal/profiled native comparisons for source-column precedence, HAVING aliases, ordinal grouping and ordering. Open-schema alias nuances and broader window/correlated scopes remain open.

Inline/named window merged-key partitions and windowed checked writes now have initial read/profile/failure/retry/rollback coverage. Direct source-free scalar/EXISTS subqueries now resolve an outer merged key in their projection and WHERE expressions. The 24-case differential matrix covers INNER/LEFT/RIGHT joins, direct and closed collection sources, scalar arithmetic/filters, EXISTS and a local FROM column that must shadow the outer key. Nested scalar scopes (including `(SELECT (SELECT k))` with RIGHT JOIN), sourceful outer-name fallback, WITH/compound scopes and inner grouping/broader ordering remain unqualified; this change does not claim general correlated USING support.


Typed direct correlation now has 18 read/profile shapes for record, boolean and binary keys across INNER/LEFT/RIGHT joins and direct/closed collection sources, plus correlated collection insertion, unique-index failure, rollback and retry. Nested/sourceful scopes remain unfinished.


Direct source-free correlation now also qualifies outer merged keys in inner ORDER BY, preserving explicit output aliases (including same-name aliases). Implicit column labels are not treated as explicit aliases during qualification. The expanded 66-shape execute/profile matrix covers bare and aliased ordering, WHERE source-before-alias behavior, independent nested EXISTS and LIMIT 0. General nested/sourceful correlation and grouped scopes remain unfinished.


Typed correlation coverage now includes 90 execute/profile shapes with bare/aliased ordering, LIMIT 0 and OFFSET 1 scalar-NULL behavior, plus ordered correlated insertion and transaction failure/retry checks.


Duplicate public key columns now have a 27-case execute/profile differential matrix for duplicates on either/both derived sources, all three supported join kinds, merged/qualified lookup and star positions. Suppression hides all duplicate public key positions on the suppressed side, matching the pinned baseline. Correlated duplicate-key derived fixtures require separate qualification: the tested native scalar variant rejects with no such column k.


Optional and explicit-null keys now match a native NULL baseline in 18 execute/profile shapes across INNER/LEFT/RIGHT joins, direct/closed sources and NULL filters. A missing key in a closed source rejects on either side without preventing later writes and rollback on that connection. Virtual-source and broader correlation qualification remain open.


Managed-index coverage compares nine join/filter shapes before/after index creation and after updates/deletes with index removal. Rollback restores data and index catalog entries. EXPLAIN confirms docs_k for the filtered inner join; this does not establish indexed lookup for the join predicate itself.


Renamed and chained CTE keys now have 27 execute/profile differential shapes across default/MATERIALIZED/NOT MATERIALIZED sources and INNER/LEFT/RIGHT joins. A merged native key uses its resolved public name instead of an SQL-quoted expression label. Materialization evaluation counts remain separate qualification.
