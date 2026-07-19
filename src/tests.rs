fn engine_with_docs(
    dir: &std::path::Path,
    mode: crate::TokenizerMode,
    docs: &[(&str, &str)],
) -> crate::Traverze {
    let engine = crate::Traverze::builder()
        .index_dir(&dir.join("index"))
        .mode(mode)
        .open()
        .unwrap();
    let files: Vec<std::path::PathBuf> = docs
        .iter()
        .map(|(name, content)| {
            let path = dir.join(name);
            std::fs::write(&path, content).unwrap();
            path
        })
        .collect();
    engine.index(&files).unwrap();
    engine
}

fn search_names(
    engine: &crate::Traverze,
    query: &str,
    mode: crate::QueryPreprocess,
) -> Vec<String> {
    let options = crate::SearchOptions {
        limit: 10,
        snippet: None,
        query_preprocess: mode,
    };
    let mut names: Vec<String> = engine
        .search(query, options)
        .unwrap()
        .iter()
        .map(|hit| {
            std::path::Path::new(&hit.path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    names.sort();
    names
}

#[cfg(not(feature = "tokenizer-lindera-ipadic"))]
#[test]
fn default_mode_is_ngram_without_lindera_feature() {
    assert_eq!(crate::default_tokenizer_mode(), crate::TokenizerMode::Ngram);
}

#[cfg(feature = "tokenizer-lindera-ipadic")]
#[test]
fn default_mode_is_lindera_with_feature() {
    assert_eq!(
        crate::default_tokenizer_mode(),
        crate::TokenizerMode::LinderaIpadic
    );
}

#[test]
fn list_files_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = crate::Traverze::builder()
        .index_dir(dir.path())
        .mode(crate::TokenizerMode::Ngram)
        .open()
        .unwrap();
    let files = engine.list().unwrap();
    assert!(files.is_empty());
}

#[test]
fn list_files_returns_indexed_paths() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("index");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    std::fs::write(&file_a, "hello world").unwrap();
    std::fs::write(&file_b, "foo bar").unwrap();

    let engine = crate::Traverze::builder()
        .index_dir(&index_dir)
        .mode(crate::TokenizerMode::Ngram)
        .open()
        .unwrap();
    let count = engine.index(&[file_a.clone(), file_b.clone()]).unwrap();
    assert_eq!(count, 2);

    let files = engine.list().unwrap();
    assert_eq!(files.len(), 2);
    // list returns sorted paths
    let canonical_a = std::fs::canonicalize(&file_a)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let canonical_b = std::fs::canonicalize(&file_b)
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(files.contains(&canonical_a));
    assert!(files.contains(&canonical_b));
}

// Issue #24: on an ngram index, auto mode must AND the query words even
// though the analyzer emits ngram tokens spanning word boundaries.
#[test]
fn auto_multiword_query_is_and_on_ngram_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::Ngram,
        &[
            ("a.txt", "abc xyz"),
            ("b.txt", "def xyz"),
            ("c.txt", "abc def xyz"),
        ],
    );

    let auto = search_names(&engine, "abc def", crate::QueryPreprocess::Auto);
    assert_eq!(auto, vec!["c.txt"]);

    // plain mode keeps Tantivy's default OR semantics
    let plain = search_names(&engine, "abc def", crate::QueryPreprocess::Plain);
    assert_eq!(plain, vec!["a.txt", "b.txt", "c.txt"]);
}

// Issue #24: the AND/OR/... operators inside the user query are tokenized
// like any other word and must be matched literally, not parsed as syntax.
#[test]
fn auto_and_keyword_is_literal_on_ngram_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::Ngram,
        &[
            ("plain.txt", "search replace tool"),
            ("with_and.txt", "search and replace tool"),
        ],
    );

    let hits = search_names(&engine, "search AND replace", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["with_and.txt"]);
}

// Issue #24: Tantivy syntax characters inside tokens must not corrupt the
// assembled query structure.
#[test]
fn auto_syntax_chars_do_not_corrupt_query_on_ngram_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::Ngram,
        &[
            ("colon.txt", "foo:bar baz"),
            ("nocolon.txt", "foo bar baz"),
            ("quoted.txt", "say \"hi\" there"),
            ("unquoted.txt", "say hi there"),
        ],
    );

    let hits = search_names(&engine, "foo:bar", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["colon.txt"]);

    let hits = search_names(&engine, "say \"hi\"", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["quoted.txt"]);
}

#[test]
fn auto_cjk_query_on_ngram_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::Ngram,
        &[
            ("date.txt", "日付を確認する"),
            ("other.txt", "無関係な内容"),
        ],
    );

    let hits = search_names(&engine, "日付", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["date.txt"]);

    // AND semantics: a query with an unknown word must not match
    let hits = search_names(&engine, "日付 未知語ワード", crate::QueryPreprocess::Auto);
    assert!(hits.is_empty());
}

#[test]
fn preprocess_auto_drops_cross_word_ngrams_and_quotes_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let engine = crate::Traverze::builder()
        .index_dir(dir.path())
        .mode(crate::TokenizerMode::Ngram)
        .open()
        .unwrap();

    let out =
        crate::preprocess_query(&engine.index, "abc def", crate::QueryPreprocess::Auto).unwrap();
    assert_eq!(
        out,
        r#""ab" AND "abc" AND "bc" AND "de" AND "def" AND "ef""#
    );

    let out = crate::preprocess_query(&engine.index, "a\"b", crate::QueryPreprocess::Auto).unwrap();
    assert_eq!(out, r#""a\"" AND "a\"b" AND "\"b""#);
}

#[cfg(feature = "tokenizer-lindera-ipadic")]
#[test]
fn auto_multiword_query_is_and_on_lindera_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::LinderaIpadic,
        &[
            ("a.txt", "abc xyz"),
            ("b.txt", "def xyz"),
            ("c.txt", "abc def xyz"),
        ],
    );

    let hits = search_names(&engine, "abc def", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["c.txt"]);
}

#[cfg(feature = "tokenizer-lindera-ipadic")]
#[test]
fn auto_cjk_query_on_lindera_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_docs(
        dir.path(),
        crate::TokenizerMode::LinderaIpadic,
        &[
            ("date.txt", "日付を確認する"),
            ("other.txt", "無関係な内容"),
        ],
    );

    let hits = search_names(&engine, "日付 確認", crate::QueryPreprocess::Auto);
    assert_eq!(hits, vec!["date.txt"]);

    let hits = search_names(&engine, "日付 未知語ワード", crate::QueryPreprocess::Auto);
    assert!(hits.is_empty());
}

// Issue #23: commit must transparently retry on transient PermissionDenied.
mod commit_retry {
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tantivy::directory::error::{DeleteError, OpenReadError, OpenWriteError};
    use tantivy::directory::{
        Directory, FileHandle, RamDirectory, WatchCallback, WatchHandle, WritePtr,
    };
    use tantivy::{Index, IndexSettings, TantivyError};

    /// Wraps a `RamDirectory` and fails the next `failures_left` segment
    /// file writes with `PermissionDenied`, simulating the antivirus
    /// interference from issue #23. Lockfiles are exempt so that writer
    /// creation (which must not be retried) always succeeds.
    #[derive(Clone, Debug)]
    struct FailingDirectory {
        inner: RamDirectory,
        failures_left: Arc<AtomicUsize>,
    }

    impl FailingDirectory {
        fn new() -> Self {
            Self {
                inner: RamDirectory::create(),
                failures_left: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn arm(&self, failures: usize) {
            self.failures_left.store(failures, Ordering::SeqCst);
        }

        fn armed(&self) -> usize {
            self.failures_left.load(Ordering::SeqCst)
        }
    }

    impl Directory for FailingDirectory {
        fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
            self.inner.get_file_handle(path)
        }

        fn delete(&self, path: &Path) -> Result<(), DeleteError> {
            self.inner.delete(path)
        }

        fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
            self.inner.exists(path)
        }

        fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
            let is_lock = path.extension().is_some_and(|ext| ext == "lock");
            if !is_lock
                && self
                    .failures_left
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                    .is_ok()
            {
                return Err(OpenWriteError::wrap_io_error(
                    std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                    path.to_path_buf(),
                ));
            }
            self.inner.open_write(path)
        }

        fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
            self.inner.atomic_read(path)
        }

        fn atomic_write(&self, path: &Path, data: &[u8]) -> std::io::Result<()> {
            self.inner.atomic_write(path, data)
        }

        fn sync_directory(&self) -> std::io::Result<()> {
            self.inner.sync_directory()
        }

        fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
            self.inner.watch(watch_callback)
        }
    }

    fn engine_on(directory: FailingDirectory) -> crate::Traverze {
        let index = Index::create(
            directory,
            crate::build_schema(false),
            IndexSettings::default(),
        )
        .unwrap();
        crate::Traverze::from_index(index, crate::TokenizerMode::Ngram).unwrap()
    }

    #[test]
    fn index_recovers_from_transient_permission_denied() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "hello world").unwrap();

        let directory = FailingDirectory::new();
        let engine = engine_on(directory.clone());
        directory.arm(1);

        let count = engine.index(&[file]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(directory.armed(), 0, "injected failure did not fire");
        assert_eq!(engine.list().unwrap().len(), 1);
    }

    #[test]
    fn remove_recovers_from_transient_permission_denied() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "hello world").unwrap();

        let directory = FailingDirectory::new();
        let engine = engine_on(directory.clone());
        engine.index(std::slice::from_ref(&file)).unwrap();
        directory.arm(1);

        let count = engine.remove(&[file]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(directory.armed(), 0, "injected failure did not fire");
        assert!(engine.list().unwrap().is_empty());
    }

    #[test]
    fn persistent_permission_denied_fails_after_retries() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "hello world").unwrap();

        let directory = FailingDirectory::new();
        let engine = engine_on(directory.clone());
        directory.arm(usize::MAX);

        let err = engine.index(&[file]).unwrap_err();
        assert!(
            err.to_string().contains("after 2 retries"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn transient_error_classification() {
        use std::io;

        let perm = || io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(crate::is_transient_commit_error(&TantivyError::IoError(
            Arc::new(perm())
        )));
        assert!(crate::is_transient_commit_error(
            &TantivyError::OpenWriteError(OpenWriteError::wrap_io_error(perm(), "seg.idx".into()))
        ));
        assert!(!crate::is_transient_commit_error(
            &TantivyError::OpenWriteError(OpenWriteError::wrap_io_error(
                io::Error::from(io::ErrorKind::NotFound),
                "seg.idx".into()
            ))
        ));
        assert!(!crate::is_transient_commit_error(
            &TantivyError::IndexAlreadyExists
        ));
    }
}

#[test]
fn list_files_excludes_removed() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("index");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    std::fs::write(&file_a, "hello").unwrap();
    std::fs::write(&file_b, "world").unwrap();

    {
        let engine = crate::Traverze::builder()
            .index_dir(&index_dir)
            .mode(crate::TokenizerMode::Ngram)
            .open()
            .unwrap();
        engine.index(&[file_a.clone(), file_b.clone()]).unwrap();
    }
    {
        let engine = crate::Traverze::builder()
            .index_dir(&index_dir)
            .mode(crate::TokenizerMode::Ngram)
            .open()
            .unwrap();
        engine.remove(&[file_a]).unwrap();

        let files = engine.list().unwrap();
        assert_eq!(files.len(), 1);
        let canonical_b = std::fs::canonicalize(&file_b)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(files[0], canonical_b);
    }
}
