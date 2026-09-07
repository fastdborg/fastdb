'use strict';
// In a distributed application, replace this checkout path with @fastdb/node.
const { AsyncDatabase, Record } = require('../../bindings/node/index.cjs');

const migrations = [{
  version: 1n,
  name: 'task tracker',
  sql: `CREATE TABLE people;
CREATE TABLE tasks;
DEFINE FIELD name ON people TYPE string REQUIRED;
DEFINE FIELD title ON tasks TYPE string REQUIRED CHECK(length(trim(title)) > 0);
DEFINE FIELD done ON tasks TYPE boolean REQUIRED;
DEFINE FIELD owner ON tasks TYPE record<people> REQUIRED;
CREATE INDEX tasks_done ON tasks(done);
CREATE TABLE task_events(task_key TEXT PRIMARY KEY, kind TEXT NOT NULL);`,
}];

async function openTracker(file) {
  const db = await AsyncDatabase.open(file);
  try { await db.migrate(migrations); return db; }
  catch (error) { await db.close(); throw error; }
}

async function addPerson(db, key, name) {
  const id = new Record('people', key);
  await db.execute('UPSERT people {id:$id,name:$name}', {$id:id, $name:name});
  return id;
}

async function addTask(db, key, title, owner) {
  const id = new Record('tasks', key);
  await db.execute('INSERT INTO tasks DOCUMENT $task', {$task: {id, title, owner, done: false}});
  return id;
}

// The caller must serialize use of this connection for this entire operation.
// A worker serializes statements, not ownership of a multi-statement transaction.
async function completeTask(db, id) {
  await db.execute('BEGIN');
  try {
    const result = await db.execute('UPDATE tasks SET done=$done WHERE id=$id AND done=$pending', {$id:id, $done:true, $pending:false});
    if (result.affected !== 1n) throw new Error('Task is missing or already complete');
    await db.execute('INSERT INTO task_events VALUES ($key,$kind)', {$key:id.key, $kind:'completed'});
    await db.execute('COMMIT');
  } catch (error) {
    try { await db.execute('ROLLBACK'); }
    catch (rollbackError) { throw new AggregateError([error, rollbackError], 'Task operation and rollback failed'); }
    throw error;
  }
}

async function listTasks(db) {
  const rows = await db.all('SELECT id,title,done,record::fetch(owner) AS person FROM tasks ORDER BY title');
  return rows.map(([id,title,done,person])=>({id,title,done,owner:person?.name ?? null}));
}

module.exports = {openTracker, addPerson, addTask, completeTask, listTasks};

if (require.main === module) {
  (async()=> {
    if (process.argv.length !== 3) throw new Error('Usage: node app.cjs /path/to/new-demo.db');
    const db = await openTracker(process.argv[2]);
    try {
      const person = await addPerson(db, 'sam', 'Sam');
      const id = new Record('tasks','first');
      if (!await db.first('SELECT id FROM tasks WHERE id=$id', {$id:id})) {
        await addTask(db,'first','Ship the embedded prototype',person);
        await completeTask(db,id);
      }
      console.log(JSON.stringify(await listTasks(db),(_,value)=>typeof value==='bigint'?value.toString():value,2));
    } finally { await db.close(); }
  })().catch(error=>{console.error(error);process.exitCode=1;});
}
