# Embedded task tracker

A runnable CommonJS application template using the native asynchronous Node client. It demonstrates versioned migrations, typed records and booleans, required fields and CHECK validation, an indexed task field, one-hop owner expansion, and a transaction spanning document and relational writes.

Build the local addon from the repository root with `fastdb/scripts/check-node.sh` using the documented Rust toolchain. Then run:

```sh
node fastdb/examples/node-task-tracker/app.cjs ./tracker-demo.db
node --test fastdb/examples/node-task-tracker/test.cjs
```

Use a new database path for the demo. It creates a person and a completed task, prints a board summary, and can be rerun without duplicating the first task. The script closes its worker in a finally block. The import points at the checkout client; a future distributed application would import `@fastdb/node` after installing an appropriate native artifact.

The exported `openTracker`, `addPerson`, `addTask`, `completeTask` and `listTasks` functions are the starting point for an application's storage layer. Keep migration version/name/source unchanged after applying it; append a new migration for later schema changes. Pass application values as parameters and use Record for collection identities. JavaScript booleans remain typed when bound; native SQL TRUE/FALSE are not a substitute for typed boolean writes in this example.

`completeTask` updates the document and inserts an audit event in one explicit transaction. Its caller must exclusively own the connection until the function returns. Worker statement ordering does not isolate concurrent callers' transactions. On failure it attempts rollback and retains both errors if cleanup also fails. A commit error can have an uncertain outcome; inspect/reconcile stored state before deciding to retry.

The smoke test forces the relational event insert to fail after the document update, checks document/index rollback, retries successfully, rejects repeat completion, and reopens the file through the migration runner. It also checks required-title validation, owner expansion and NDJSON export. The example is included in the scoped Node check; it is an application starting point, not external pilot or production-release evidence.

See the [AI application guide](../../docs/ai-application-guide.md) for a runnable storage-layer walkthrough and suggested application-agent instructions.
