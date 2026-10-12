mod admission;
#[cfg(unix)]
pub mod http;
#[cfg(all(test, unix))]
mod recovery;
mod request;
use admission::Admission;
use fastdb::{
    CancellationToken, Connection, Database, Parameters, RequestBudget, TransactionState,
};
use request::{PreparedStatement, Request};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::OpenOptions,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, TryLockError,
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Error {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.chars().take(2048).collect(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
impl From<fastdb::Error> for Error {
    fn from(error: fastdb::Error) -> Self {
        Self::new(error.code(), &error.to_string())
    }
}
impl From<fastdb::RequestError> for Error {
    fn from(error: fastdb::RequestError) -> Self {
        Self::new(&error.code, &error.message)
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::new("FDB_STORAGE", "service filesystem operation failed")
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("FDB_PROTOCOL", "service JSON conversion failed")
    }
}

pub struct DatabaseService {
    database: Database,
    database_id: String,
    publication: Mutex<Publication>,
    schema: RwLock<()>,
    admission: Arc<Admission>,
    fenced: Arc<AtomicBool>,
}

struct Publication {
    connection: Connection,
    sequence: u64,
}

impl DatabaseService {
    pub fn create(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(Error::new(
                "FDB_VALIDATION",
                "service creation requires an absolute path",
            ));
        }
        let mut sidecars = Vec::new();
        for suffix in ["-wal", "-shm", "-log", "-journal"] {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            sidecars.push(std::path::PathBuf::from(name));
        }
        sidecars.push(path.with_extension("db-log"));
        for sidecar in sidecars {
            match std::fs::symlink_metadata(sidecar) {
                Ok(_) => {
                    return Err(Error::new(
                        "FDB_CONFLICT",
                        "database sidecar exists; recover it before creating a file",
                    ))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)?.sync_all()?;
        let database = Database::open(
            path.to_str()
                .ok_or_else(|| Error::new("FDB_VALIDATION", "database path must be UTF-8"))?,
        )?;
        let connection = database.connect()?;
        connection.execute("PRAGMA journal_mode=mvcc", &Parameters::new())?;
        drop(connection);
        let service = Self::initialize(database)?;
        #[cfg(unix)]
        std::fs::File::open(path.parent().ok_or_else(|| {
            Error::new(
                "FDB_VALIDATION",
                "database path requires a parent directory",
            )
        })?)?
        .sync_all()?;
        Ok(service)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    "FDB_NOT_FOUND",
                    "service requires an existing database file",
                )
            } else {
                error.into()
            }
        })?;
        #[cfg(unix)]
        let one_link = std::os::unix::fs::MetadataExt::nlink(&metadata) == 1;
        #[cfg(not(unix))]
        let one_link = true;
        if !metadata.is_file() || !one_link {
            return Err(Error::new(
                "FDB_VALIDATION",
                "service requires a regular database file with one link; use its original path",
            ));
        }
        let database = Database::open_for_service(
            path.to_str()
                .ok_or_else(|| Error::new("FDB_VALIDATION", "database path must be UTF-8"))?,
        )?;
        Self::initialize(database)
    }

    fn initialize(database: Database) -> Result<Self> {
        let connection = database.connect()?;
        let catalog = connection.initialize_service_catalog(&uuid::Uuid::new_v4().to_string())?;
        Ok(Self {
            database,
            database_id: catalog.database_id,
            publication: Mutex::new(Publication {
                connection,
                sequence: catalog.sequence,
            }),
            schema: RwLock::new(()),
            admission: Arc::default(),
            fenced: Arc::default(),
        })
    }

    pub fn info(&self) -> Result<Value> {
        let deadline = deadline();
        self.check_fence()?;
        let _permit = self.admission.acquire(deadline)?;
        let publication = self.lock_before(deadline, || self.publication.try_lock())?;
        Ok(
            json!({"id":self.database_id,"sequence":publication.sequence,"serviceVersion":1,"journalMode":"mvcc",
            "capabilities":{"readVersion":2,"atomicWrites":true,"replayWindow":64,"exactSequenceCondition":true,"metadata":true,"checkedWork":true,"foreignKeys":true},
            "limits":{"maxRequestBytes":65536,"maxStatements":32,"maxRows":1000,"maxResultBytes":262144,"maxResponseBytes":524288,"activeRequests":4,"queuedRequests":32,"connections":5,"executionTimeoutMs":15000,"rowsRead":1000000,"rowMutations":100000,"vmSteps":10000000}}),
        )
    }

    pub fn read(&self, body: &str) -> Result<String> {
        let started = deadline();
        self.check_fence()?;
        let request = Request::parse(body, true)?;
        let statements = request.prepare(true)?;
        let _permit = self.admission.acquire(started)?;
        let _schema = self.lock_before(started, || self.schema.try_read())?;
        self.check_fence()?;
        let scope = self.connection()?;
        scope.begin(false)?;
        let operation = (|| {
            let sequence = scope.connection.service_catalog()?.sequence;
            if request.after_sequence > sequence {
                return Err(Error::new(
                    "FDB_CONFLICT",
                    "requested sequence is ahead of this database",
                ));
            }
            let mut budget = request.budget()?;
            let results = execute(
                &scope.connection,
                &statements,
                &mut budget,
                &CancellationToken::with_deadline(started),
            )?;
            Ok(serde_json::to_string(
                &json!({"readVersion":2,"requestId":request.request_id,"sequence":sequence,"results":results,"executionWork":budget}),
            )?)
        })();
        scope.rollback()?;
        operation
    }

    pub fn query(&self, body: &str) -> Result<String> {
        let started = deadline();
        self.check_fence()?;
        let request = Request::parse(body, false)?;
        let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
        let statements = request.prepare(false).map_err(Into::into);
        let schema_change = statements.as_ref().map_or(true, |s| {
            s.iter()
                .any(|s| s.kind == fastdb::RequestStatementKind::Schema)
        });
        let _permit = self.admission.acquire(started)?;
        if !schema_change {
            let _schema = self.lock_before(started, || self.schema.try_read())?;
            if let Some(reply) = self.query_locked(&request, &hash, &statements, started, false)? {
                return Ok(reply);
            }
        }
        let _schema = self.lock_before(started, || self.schema.try_write())?;
        Ok(self
            .query_locked(&request, &hash, &statements, started, true)?
            .expect("an exclusive transaction never asks for another exclusive retry"))
    }

    fn query_locked(
        &self,
        request: &Request,
        hash: &str,
        statements: &Result<Vec<PreparedStatement>>,
        deadline: Instant,
        schema_change: bool,
    ) -> Result<Option<String>> {
        self.check_fence()?;
        {
            let mut publication = self.lock_before(deadline, || self.publication.try_lock())?;
            if let Some(reply) = self.replay(&publication, request, hash)? {
                return Ok(Some(reply));
            }
            check_position(request, publication.sequence)?;
            if request
                .if_sequence
                .is_some_and(|expected| expected != publication.sequence)
            {
                return self
                    .failure(
                        &mut publication,
                        request,
                        hash,
                        &Error::new("FDB_CONFLICT", "sequence changed; refresh before saving"),
                        request.budget()?,
                    )
                    .map(Some);
            }
        }
        let statements = match statements {
            Ok(statements) => statements,
            Err(error) => {
                let mut publication = self.lock_before(deadline, || self.publication.try_lock())?;
                if let Some(reply) = self.replay(&publication, request, hash)? {
                    return Ok(Some(reply));
                }
                check_position(request, publication.sequence)?;
                return self
                    .failure(&mut publication, request, hash, error, request.budget()?)
                    .map(Some);
            }
        };
        let token = CancellationToken::with_deadline(deadline);
        let mut budget = request.budget()?;
        for _ in 0..16 {
            budget = request.budget()?;
            self.check_fence()?;
            let scope = self.connection()?;
            scope.begin(schema_change)?;
            let snapshot = scope.connection.service_catalog()?.sequence;
            let result =
                scope
                    .connection
                    .execute_request_statements(statements, &mut budget, &token);
            if !schema_change
                && result
                    .as_ref()
                    .is_err_and(fastdb::Error::requires_exclusive_transaction)
            {
                scope.rollback()?;
                return Ok(None);
            }
            let result = result.map_err(Error::from);
            let mut publication = self.lock_before(deadline, || self.publication.try_lock())?;
            if let Some(reply) = self.replay(&publication, request, hash)? {
                scope.rollback()?;
                return Ok(Some(reply));
            }
            check_position(request, publication.sequence)?;
            if request
                .if_sequence
                .is_some_and(|expected| expected != publication.sequence)
            {
                scope.rollback()?;
                return self
                    .failure(
                        &mut publication,
                        request,
                        hash,
                        &Error::new("FDB_CONFLICT", "sequence changed; refresh before saving"),
                        budget,
                    )
                    .map(Some);
            }
            let results = match result {
                Ok(results) => results,
                Err(error) => {
                    scope.rollback()?;
                    return self
                        .failure(&mut publication, request, hash, &error, budget)
                        .map(Some);
                }
            };
            if snapshot != publication.sequence {
                scope.rollback()?;
                continue;
            }
            let response = serde_json::to_string(
                &json!({"requestId":request.request_id,"sequence":publication.sequence+1,"results":results,"executionWork":budget}),
            )?;
            if scope
                .connection
                .publish_service_receipt(publication.sequence, &request.request_id, hash, &response)
                .is_err()
            {
                scope.rollback()?;
                return Err(self.fence());
            }
            if let Err(error) = scope.commit() {
                if matches!(
                    error.code.as_str(),
                    "FDB_WRITE_CONFLICT" | "FDB_BUSY_SNAPSHOT" | "FDB_BUSY"
                ) {
                    scope.rollback()?;
                    return self
                        .failure(&mut publication, request, hash, &error, budget)
                        .map(Some);
                }
                return Err(self.fence());
            }
            publication.sequence += 1;
            return Ok(Some(response));
        }
        let mut publication = self.lock_before(deadline, || self.publication.try_lock())?;
        if let Some(reply) = self.replay(&publication, request, hash)? {
            return Ok(Some(reply));
        }
        check_position(request, publication.sequence)?;
        self.failure(
            &mut publication,
            request,
            hash,
            &Error::new(
                "FDB_WRITE_CONFLICT",
                "publication changed during the bounded retry attempts",
            ),
            budget,
        )
        .map(Some)
    }

    fn replay(
        &self,
        publication: &Publication,
        request: &Request,
        hash: &str,
    ) -> Result<Option<String>> {
        match publication.connection.service_receipt(&request.request_id) {
            Ok(Some(receipt))
                if receipt.request_hash == hash && receipt.sequence <= publication.sequence =>
            {
                Ok(Some(receipt.response))
            }
            Ok(Some(_)) => Err(Error::new(
                "FDB_CONFLICT",
                "request ID was reused with a different body",
            )),
            Ok(None) => Ok(None),
            Err(_) => Err(self.fence()),
        }
    }

    fn failure(
        &self,
        publication: &mut Publication,
        request: &Request,
        hash: &str,
        error: &Error,
        budget: RequestBudget,
    ) -> Result<String> {
        if matches!(error.code.as_str(), "FDB_ROLLBACK" | "FDB_STORAGE") {
            return Err(self.fence());
        }
        let response = serde_json::to_string(
            &json!({"requestId":request.request_id,"sequence":publication.sequence+1,"error":{"code":error.code,"message":error.message},"executionWork":budget}),
        )?;
        if publication
            .connection
            .execute("BEGIN CONCURRENT", &Parameters::new())
            .is_err()
        {
            return Err(self.fence());
        }
        let outcome = publication
            .connection
            .publish_service_receipt(publication.sequence, &request.request_id, hash, &response)
            .and_then(|()| {
                publication
                    .connection
                    .execute("COMMIT", &Parameters::new())
                    .map(|_| ())
            });
        if outcome.is_err() {
            let _ = publication
                .connection
                .execute("ROLLBACK", &Parameters::new());
            return Err(self.fence());
        }
        publication.sequence += 1;
        Ok(response)
    }

    fn connection(&self) -> Result<Scope> {
        let connection = self
            .database
            .connect()?
            .with_write_buffer_limits(fastdb::ResultLimits {
                max_rows: 10_000,
                max_payload_bytes: 64 * 1024 * 1024,
            });
        connection.execute("PRAGMA foreign_keys=ON", &Parameters::new())?;
        Ok(Scope {
            connection,
            fenced: self.fenced.clone(),
        })
    }

    fn check_fence(&self) -> Result<()> {
        if self.fenced.load(Ordering::SeqCst) {
            Err(self.fence())
        } else {
            Ok(())
        }
    }

    fn lock_before<T>(
        &self,
        deadline: Instant,
        mut acquire: impl FnMut() -> std::result::Result<T, TryLockError<T>>,
    ) -> Result<T> {
        loop {
            self.check_fence()?;
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "FDB_CANCELLED",
                    "request deadline expired waiting for a service lock",
                ));
            }
            match acquire() {
                Ok(value) => return Ok(value),
                Err(TryLockError::Poisoned(_)) => return Err(self.fence()),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
    fn fence(&self) -> Error {
        self.fenced.store(true, Ordering::SeqCst);
        Error::new(
            "FDB_STORAGE",
            "service outcome is uncertain; stop and recover the database before continuing",
        )
    }
    pub fn begin_shutdown(&self) {
        self.admission.stop();
    }
}

struct Scope {
    connection: Connection,
    fenced: Arc<AtomicBool>,
}
impl Scope {
    fn begin(&self, schema_change: bool) -> Result<()> {
        self.connection.execute(
            if schema_change {
                "BEGIN IMMEDIATE"
            } else {
                "BEGIN CONCURRENT"
            },
            &Parameters::new(),
        )?;
        Ok(())
    }
    fn commit(&self) -> Result<()> {
        self.connection.execute("COMMIT", &Parameters::new())?;
        Ok(())
    }
    fn rollback(&self) -> Result<()> {
        if self.connection.transaction_state() == TransactionState::Active
            && self
                .connection
                .execute("ROLLBACK", &Parameters::new())
                .is_err()
        {
            self.fenced.store(true, Ordering::SeqCst);
            return Err(Error::new(
                "FDB_STORAGE",
                "request rollback could not be confirmed",
            ));
        }
        Ok(())
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let _ = self.rollback();
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(15)
}
fn check_position(request: &Request, sequence: u64) -> Result<()> {
    if sequence > request::MAX_SEQUENCE {
        return Err(Error::new("FDB_LIMIT", "publication sequence is exhausted"));
    }
    if request.after_sequence > sequence || sequence - request.after_sequence >= 64 {
        return Err(Error::new(
            "FDB_CONFLICT",
            "request retry window expired or is ahead of the database; reconcile the saved request",
        ));
    }
    Ok(())
}

fn execute(
    connection: &Connection,
    statements: &[PreparedStatement],
    budget: &mut RequestBudget,
    token: &CancellationToken,
) -> Result<Vec<Value>> {
    Ok(connection.execute_request_statements(statements, budget, token)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_deadline_interrupts_engine_work_and_connection_remains_usable() {
        let directory = tempfile::tempdir().unwrap();
        let service = DatabaseService::create(&directory.path().join("app.fastdb")).unwrap();
        let request = Request::parse(
            &json!({"readVersion":2,"requestId":"deadline-probe","afterSequence":0,"statements":[{"sql":"WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n"}]}).to_string(),
            true,
        ).unwrap();
        let statements = request.prepare(true).unwrap();
        let scope = service.connection().unwrap();
        scope.begin(false).unwrap();
        let started = Instant::now();
        let error = execute(
            &scope.connection,
            &statements,
            &mut request.budget().unwrap(),
            &CancellationToken::with_deadline(started + Duration::from_millis(20)),
        )
        .unwrap_err();
        assert_eq!(error.code, "FDB_CANCELLED");
        assert!(started.elapsed() < Duration::from_secs(2));
        scope.rollback().unwrap();
        scope
            .connection
            .execute("SELECT 1", &Parameters::new())
            .unwrap();
        assert_eq!(service.info().unwrap()["sequence"], 0);
    }

    #[test]
    fn service_lock_waits_obey_the_request_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let service = DatabaseService::create(&directory.path().join("app.fastdb")).unwrap();
        let publication = service.publication.lock().unwrap();
        let started = Instant::now();
        let error = service
            .lock_before(started + Duration::from_millis(20), || {
                service.publication.try_lock()
            })
            .err()
            .unwrap();
        assert_eq!(error.code, "FDB_CANCELLED");
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(publication);
        assert_eq!(service.info().unwrap()["sequence"], 0);
    }

    #[test]
    fn readers_keep_their_snapshot_during_writes_and_block_schema_changes() {
        let directory = tempfile::tempdir().unwrap();
        let service =
            Arc::new(DatabaseService::create(&directory.path().join("app.fastdb")).unwrap());
        service.query(&json!({"requestId":"setup","afterSequence":0,"statements":[{"sql":"CREATE TABLE rows (id INTEGER PRIMARY KEY,n INTEGER)"},{"sql":"INSERT INTO rows VALUES(1,0)"}]}).to_string()).unwrap();
        let reader = service.schema.read().unwrap();
        let scope = service.connection().unwrap();
        scope.begin(false).unwrap();
        assert_eq!(
            scope
                .connection
                .execute("SELECT n FROM rows", &Parameters::new())
                .unwrap()
                .rows,
            vec![vec![fastdb::Value::Integer(0)]]
        );
        let writer = {
            let service = service.clone();
            std::thread::spawn(move || {
                service.query(&json!({"requestId":"write","afterSequence":1,"statements":[{"sql":"UPDATE rows SET n=1 WHERE id=1"}]}).to_string()).unwrap()
            })
        };
        let reply: Value = serde_json::from_str(&writer.join().unwrap()).unwrap();
        assert!(reply["error"].is_null(), "{reply}");
        assert_eq!(
            scope
                .connection
                .execute("SELECT n FROM rows", &Parameters::new())
                .unwrap()
                .rows,
            vec![vec![fastdb::Value::Integer(0)]]
        );
        assert_eq!(scope.connection.service_catalog().unwrap().sequence, 1);
        let (finished, result) = std::sync::mpsc::channel();
        let schema = {
            let service = service.clone();
            std::thread::spawn(move || {
                finished.send(service.query(&json!({"requestId":"schema","afterSequence":2,"statements":[{"sql":"ALTER TABLE rows ADD COLUMN added INTEGER DEFAULT 7"}]}).to_string())).unwrap()
            })
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while service.admission.counts().0 == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(result.try_recv().is_err());
        scope.rollback().unwrap();
        drop(scope);
        drop(reader);
        let reply: Value = serde_json::from_str(
            &result
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(reply["error"].is_null(), "{reply}");
        schema.join().unwrap();
        assert_eq!(service.info().unwrap()["sequence"], 3);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_waits_for_admitted_work_after_the_client_disconnects() {
        use std::io::Write;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.fastdb");
        let service = Arc::new(DatabaseService::create(&path).unwrap());
        let token_path = directory.path().join("token");
        let authorization = crate::http::Token::load_or_create(&token_path).unwrap();
        let token = std::fs::read_to_string(&token_path).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (locked, lock_ready) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let owner = {
            let service = service.clone();
            std::thread::spawn(move || {
                let _publication = service.publication.lock().unwrap();
                locked.send(()).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(5));
            })
        };
        lock_ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let mut server = tokio::spawn(crate::http::serve(
            listener,
            service.clone(),
            authorization,
            async {
                let _ = stopped.await;
            },
        ));
        let body = json!({"requestId":"detached","afterSequence":0,"statements":[{"sql":"CREATE TABLE docs;"},{"sql":"INSERT INTO docs {id:docs:p1,n:1}"}]}).to_string();
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket.write_all(format!("POST /v1/database/query HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",token.trim(),body.len()).as_bytes()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while service.admission.counts().0 == 0 {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        drop(socket);
        tokio::time::sleep(Duration::from_millis(20)).await;
        stop.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut server)
                .await
                .is_err(),
            "server returned while an admitted database job was still running"
        );
        release.send(()).unwrap();
        owner.join().unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(service);
        let service = DatabaseService::open(&path).unwrap();
        assert_eq!(service.info().unwrap()["sequence"], 1);
        let reply: Value = serde_json::from_str(&service.query(&body).unwrap()).unwrap();
        assert!(reply["error"].is_null(), "{reply}");
    }
}
