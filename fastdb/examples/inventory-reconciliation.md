# Inventory reconciliation

Run the companion script against a new database:

```sh
cargo run --locked -p fastdb-cli -- inventory.db < fastdb/examples/inventory-reconciliation.fastql
```

The example changes P1 from 10 to 12 units and P2 from 5 to 2, records two events
with typed inventory references, clears the adjustment table and commits. The
inventory and events are collections; the adjustment staging table uses ordinary
SQL columns and a primary key. One transaction contains the event inserts and
stock updates.

The staging primary key prevents duplicate adjustments for one SKU, avoiding
plan-dependent selection among joined source matches. The quantity validator
rejects negative final stock. In an application, explicitly roll back the whole
transaction if any step fails; statement rollback alone would leave preceding
event inserts pending. Inspect the error's transaction report because some
native engine errors abort the entire transaction themselves.

This sample accepts known SKUs. An inner join leaves unknown adjustment SKUs
unmatched; an application should reject or separately report them before
clearing staging. It does not implement batch idempotency, competing inventory
reservations or automatic retries. Replaying a batch without an application
idempotency key could apply its deltas twice. The CLI regression runs the actual
script and checks final stock and event values.
