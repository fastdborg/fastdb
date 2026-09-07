'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {openTracker,addPerson,addTask,completeTask,listTasks}=require('./app.cjs');

test('task tracker persists linked tasks and atomic completion events',async()=>{
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'fastdb-tracker-'));
  const file=path.join(dir,'tracker.db');
  let db;
  try {
    db=await openTracker(file);
    const owner=await addPerson(db,'sam','Sam');
    await assert.rejects(addTask(db,'invalid','  ',owner));
    const id=await addTask(db,'first','First task',owner);
    // Force the second write to fail after the document update has succeeded.
    await db.execute("INSERT INTO task_events VALUES ('first','conflict')");
    await assert.rejects(completeTask(db,id));
    assert.equal((await listTasks(db))[0].done,false);
    assert.equal((await db.checkCollectionIntegrity('tasks')).indexEntries,1n);
    await db.execute("DELETE FROM task_events WHERE task_key='first'");
    await completeTask(db,id);
    await assert.rejects(completeTask(db,id),/already complete/);
    assert.deepEqual((await listTasks(db)).map(({title,done,owner})=>({title,done,owner})),[{title:'First task',done:true,owner:'Sam'}]);
    assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM task_events'),[1n]);
    await db.close(); db=await openTracker(file);
    assert.equal((await listTasks(db))[0].done,true);
    assert.equal((await db.checkCollectionIntegrity('tasks')).documents,1n);
    assert.match(await db.exportDocuments('tasks','ndjson'),/First task/);
  } finally {if(db)await db.close();fs.rmSync(dir,{recursive:true,force:true});}
});


test('tracker initialization retains migration and cleanup failures',async()=>{
  const {AsyncDatabase}=require('../../bindings/node/index.cjs');
  const originalOpen=AsyncDatabase.open;
  try {
    for(const failClose of [false,true]) {
      const migrationError=Object.assign(new Error('migration failed'),{code:'FDB_MIGRATION'});
      const closeError=Object.assign(new Error('close failed'),{code:'FDB_WORKER'});
      let closed=0;
      AsyncDatabase.open=async()=>({
        migrate:async migrations=>{assert.equal(migrations[0].version,1n);throw migrationError;},
        close:async()=>{closed++;if(failClose)throw closeError;},
      });
      await assert.rejects(openTracker('unused'),error=>{
        if(!failClose)return error===migrationError;
        assert(error instanceof AggregateError);
        assert.deepEqual(error.errors,[migrationError,closeError]);
        return true;
      });
      assert.equal(closed,1);
    }
  } finally {AsyncDatabase.open=originalOpen;}
});
