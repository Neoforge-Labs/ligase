//! The object-store backend, and what a build without it says instead.
//!
//! Compiled only under the `remote-store` feature. A build without it still
//! *parses* an `s3://` URL (see [`super::Backend`]) and fails here, with a
//! message about the build rather than about the URL — because a user whose
//! URL is right and whose wheel is wrong needs to be sent to the wheel.

#[cfg(not(feature = "remote-store"))]
use super::{BlobStore, StoreError};

/// What to tell someone whose build cannot open the store they asked for.
///
/// Names the cargo feature and the extra, because those are the two things
/// they have to act on, and says what a build *can* do without either.
#[cfg(not(feature = "remote-store"))]
const WITHOUT: &str = "this build of Ligase has no remote-store support. \
     Install `ligase-bio[remote-store]` from source, or build the engine \
     with `--features remote-store`. A `file://` store needs neither and \
     works in every build";

/// Open the object store `url` names.
///
/// # Errors
///
/// [`StoreError::Unsupported`] in a build without the feature, or if the
/// URL cannot be turned into a client.
#[cfg(not(feature = "remote-store"))]
pub fn open(url: &str) -> Result<Box<dyn BlobStore + Send + Sync>, StoreError> {
    Err(StoreError::Unsupported {
        url: url.to_owned(),
        reason: WITHOUT.to_owned(),
    })
}

#[cfg(feature = "remote-store")]
pub use enabled::open;

#[cfg(feature = "remote-store")]
mod enabled {
    use std::path::Path;

    use object_store::path::Path as StorePath;
    use object_store::{ObjectStore, ObjectStoreExt as _};

    use super::super::{BlobStore, ObjectKey, StoreError};

    /// Above this, an object is uploaded in parts rather than in one piece.
    ///
    /// A step's output can be a forty-gigabyte BAM, and a single `put`
    /// holds the whole body in memory on the way out.
    const MULTIPART_ABOVE: u64 = 16 * 1024 * 1024;

    /// How much of a large object is read at a time.
    const CHUNK: usize = 8 * 1024 * 1024;

    /// An `object_store` bucket, driven synchronously.
    ///
    /// The engine's executor is a rayon thread pool and touches no async
    /// runtime anywhere else (D27, and the note beside `ureq` in
    /// `Cargo.toml`). `object_store` is async-only, so one runtime lives
    /// here and nowhere else, and every call enters it with `block_on`.
    /// Multi-threaded rather than current-thread because several rayon
    /// workers call in at once, which a current-thread runtime cannot
    /// serve.
    struct Bucket {
        store: Box<dyn ObjectStore>,
        prefix: StorePath,
        runtime: tokio::runtime::Runtime,
        url: String,
    }

    /// Open the object store `url` names.
    ///
    /// Credentials, endpoint and region come from the provider's own
    /// environment — `AWS_ACCESS_KEY_ID`, `AWS_ENDPOINT_URL`,
    /// `GOOGLE_SERVICE_ACCOUNT`, `AZURE_STORAGE_ACCOUNT_NAME` and the rest
    /// — never from Ligase's configuration (D43). A user whose
    /// `aws s3 ls` works has this working.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unsupported`] if the URL cannot be turned into a
    /// client, which is nearly always a missing credential or endpoint.
    pub fn open(url: &str) -> Result<Box<dyn BlobStore + Send + Sync>, StoreError> {
        let unsupported = |reason: String| StoreError::Unsupported {
            url: url.to_owned(),
            reason,
        };

        let parsed = url::Url::parse(url).map_err(|error| unsupported(error.to_string()))?;
        let (store, prefix) = object_store::parse_url(&parsed)
            .map_err(|error| unsupported(format!("{error}. Credentials and endpoint come from the provider's own environment; Ligase reads none of its own")))?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| unsupported(error.to_string()))?;

        Ok(Box::new(Bucket {
            store,
            prefix,
            runtime,
            url: url.to_owned(),
        }))
    }

    impl Bucket {
        /// Where an object lives in the bucket.
        ///
        /// Fanned out over the first byte of the hash exactly as the
        /// filesystem backend fans out, so one store can be read as either
        /// — an `s3 sync` of a local store into a bucket is a working
        /// remote tier, and a bucket mounted on a filesystem is one too.
        fn path_of(&self, key: &ObjectKey) -> StorePath {
            let hex = key.hash.to_hex();
            self.prefix
                .clone()
                .join(&hex[..2])
                .join(format!("{}{}", &hex[2..], key.suffix))
        }

        fn io(&self, key: &ObjectKey, error: &dyn std::fmt::Display) -> StoreError {
            StoreError::Io {
                path: std::path::PathBuf::from(format!("{}/{}", self.url, self.path_of(key))),
                source: std::io::Error::other(error.to_string()),
            }
        }
    }

    impl BlobStore for Bucket {
        fn has(&self, key: &ObjectKey) -> Result<bool, StoreError> {
            let path = self.path_of(key);
            match self.runtime.block_on(self.store.head(&path)) {
                Ok(_) => Ok(true),
                Err(object_store::Error::NotFound { .. }) => Ok(false),
                Err(error) => Err(self.io(key, &error)),
            }
        }

        fn get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
            let path = self.path_of(key);
            let read = self
                .runtime
                .block_on(async { self.store.get(&path).await?.bytes().await });
            match read {
                Ok(bytes) => Ok(Some(bytes.to_vec())),
                Err(object_store::Error::NotFound { .. }) => Ok(None),
                Err(error) => Err(self.io(key, &error)),
            }
        }

        fn put(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), StoreError> {
            let path = self.path_of(key);
            let payload = object_store::PutPayload::from(bytes.to_vec());
            self.runtime
                .block_on(self.store.put(&path, payload))
                .map(|_| ())
                .map_err(|error| self.io(key, &error))
        }

        fn put_from_path(&self, key: &ObjectKey, from: &Path) -> Result<(), StoreError> {
            use std::io::Read as _;

            let length = std::fs::metadata(from)
                .map_err(|source| StoreError::Io {
                    path: from.to_path_buf(),
                    source,
                })?
                .len();
            if length <= MULTIPART_ABOVE {
                let bytes = std::fs::read(from).map_err(|source| StoreError::Io {
                    path: from.to_path_buf(),
                    source,
                })?;
                return self.put(key, &bytes);
            }

            let path = self.path_of(key);
            let mut file = std::fs::File::open(from).map_err(|source| StoreError::Io {
                path: from.to_path_buf(),
                source,
            })?;
            let mut upload = self
                .runtime
                .block_on(self.store.put_multipart(&path))
                .map_err(|error| self.io(key, &error))?;

            let mut buffer = vec![0_u8; CHUNK];
            loop {
                let read = file.read(&mut buffer).map_err(|source| StoreError::Io {
                    path: from.to_path_buf(),
                    source,
                })?;
                if read == 0 {
                    break;
                }
                let part = object_store::PutPayload::from(buffer[..read].to_vec());
                self.runtime
                    .block_on(upload.put_part(part))
                    .map_err(|error| self.io(key, &error))?;
            }
            self.runtime
                .block_on(upload.complete())
                .map(|_| ())
                .map_err(|error| self.io(key, &error))
        }

        fn put_file(&self, staged: &Path, suffix: &str) -> Result<ObjectKey, StoreError> {
            let hash =
                crate::hash::ContentHash::of_file(staged).map_err(|source| StoreError::Io {
                    path: staged.to_path_buf(),
                    source,
                })?;
            let key = ObjectKey::new(hash, suffix);
            self.put_from_path(&key, staged)?;
            std::fs::remove_file(staged).map_err(|source| StoreError::Io {
                path: staged.to_path_buf(),
                source,
            })?;
            Ok(key)
        }

        fn put_file_as(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError> {
            let found =
                crate::hash::ContentHash::of_file(staged).map_err(|source| StoreError::Io {
                    path: staged.to_path_buf(),
                    source,
                })?;
            if found != key.hash {
                return Err(StoreError::Mismatch {
                    expected: key.hash,
                    found,
                    at: staged.to_path_buf(),
                });
            }
            self.put_from_path(key, staged)?;
            std::fs::remove_file(staged).map_err(|source| StoreError::Io {
                path: staged.to_path_buf(),
                source,
            })
        }

        fn local_path(&self, _: &ObjectKey) -> Result<Option<std::path::PathBuf>, StoreError> {
            // A bucket is not a filesystem. The tier above answers this by
            // promoting into its local store first, which is the whole
            // reason this returns an `Option`.
            Ok(None)
        }
    }
}
