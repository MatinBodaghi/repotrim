use crate::{
    loader::IGNORED_DIRS, EngineError, LoadedRepository, MultiplexGraph, SupportedLanguage,
};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, RwLock};
use std::time::Duration;

/// Events emitted by the repository watcher during file system modifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    /// A supported file was created or modified and incrementally patched.
    FilePatched { path: PathBuf },
    /// A supported file was removed and unindexed.
    FileRemoved { path: PathBuf },
    /// The multiplex graph was rebuilt after one or more file changes.
    GraphUpdated {
        num_symbols: usize,
        num_edges: usize,
    },
}

enum InternalMsg {
    Notify(notify::Result<Event>),
    Stop,
}

/// Inspects whether a given path is located within an ignored directory (e.g. .git, target).
pub fn is_ignored_path(path: &Path, root: &Path) -> bool {
    let rel_path = if let Ok(p) = path.strip_prefix(root) {
        p
    } else if let Ok(c_root) = std::fs::canonicalize(root) {
        path.strip_prefix(&c_root).unwrap_or(path)
    } else {
        path
    };
    for comp in rel_path.components() {
        if let std::path::Component::Normal(c) = comp {
            let s = c.to_string_lossy();
            if IGNORED_DIRS.contains(&s.as_ref()) {
                return true;
            }
        }
    }
    false
}

/// Live background file watcher that tracks filesystem changes, debounces rapid writes,
/// incrementally updates in-memory ASTs, and keeps the MultiplexGraph synchronized.
pub struct RepositoryWatcher {
    _watcher: RecommendedWatcher,
    stop_tx: Option<mpsc::Sender<InternalMsg>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    repo: Arc<RwLock<LoadedRepository>>,
    graph: Option<Arc<RwLock<MultiplexGraph>>>,
}

impl RepositoryWatcher {
    /// Starts watching the repository with default 75ms debouncing.
    pub fn start(
        repo: Arc<RwLock<LoadedRepository>>,
        graph: Option<Arc<RwLock<MultiplexGraph>>>,
        event_tx: Option<mpsc::Sender<WatcherEvent>>,
    ) -> Result<Self, EngineError> {
        Self::start_with_debounce(repo, graph, event_tx, Duration::from_millis(75))
    }

    /// Starts watching the repository with a custom debounce duration.
    pub fn start_with_debounce(
        repo: Arc<RwLock<LoadedRepository>>,
        graph: Option<Arc<RwLock<MultiplexGraph>>>,
        event_tx: Option<mpsc::Sender<WatcherEvent>>,
        debounce: Duration,
    ) -> Result<Self, EngineError> {
        let root_path = {
            let r = repo
                .read()
                .map_err(|_| EngineError::WatcherError("Lock poisoned".into()))?;
            r.root_path.clone()
        };

        let (internal_tx, internal_rx) = mpsc::channel();
        let notify_tx = internal_tx.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = notify_tx.send(InternalMsg::Notify(res));
            },
            Config::default(),
        )?;

        watcher.watch(&root_path, RecursiveMode::Recursive)?;

        let repo_clone = Arc::clone(&repo);
        let graph_clone = graph.as_ref().map(Arc::clone);
        let root_clone = root_path.clone();

        let join_handle = std::thread::Builder::new()
            .name("repotrim-watcher".into())
            .spawn(move || {
                run_watcher_loop(
                    internal_rx,
                    repo_clone,
                    graph_clone,
                    root_clone,
                    event_tx,
                    debounce,
                );
            })
            .map_err(EngineError::IoError)?;

        Ok(Self {
            _watcher: watcher,
            stop_tx: Some(internal_tx),
            join_handle: Some(join_handle),
            repo,
            graph,
        })
    }

    /// Returns a reference to the shared in-memory LoadedRepository.
    pub fn repo(&self) -> Arc<RwLock<LoadedRepository>> {
        Arc::clone(&self.repo)
    }

    /// Returns a reference to the optional shared MultiplexGraph.
    pub fn graph(&self) -> Option<Arc<RwLock<MultiplexGraph>>> {
        self.graph.as_ref().map(Arc::clone)
    }

    /// Checks if the background worker thread is still actively running.
    pub fn is_running(&self) -> bool {
        self.join_handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Manually triggers an immediate incremental file patch and updates the graph if changed.
    pub fn patch_now<P: AsRef<Path>>(&self, path: P) -> Result<bool, EngineError> {
        let mut repo = self
            .repo
            .write()
            .map_err(|_| EngineError::WatcherError("Lock poisoned".into()))?;
        let changed = repo.patch_file(path)?;
        if changed {
            if let Some(ref graph_arc) = self.graph {
                let new_graph = repo.build_graph();
                let mut graph_lock = graph_arc
                    .write()
                    .map_err(|_| EngineError::WatcherError("Lock poisoned".into()))?;
                *graph_lock = new_graph;
            }
        }
        Ok(changed)
    }

    /// Stops the watcher background loop and joins the worker thread.
    pub fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(InternalMsg::Stop);
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RepositoryWatcher {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(InternalMsg::Stop);
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_watcher_loop(
    rx: mpsc::Receiver<InternalMsg>,
    repo: Arc<RwLock<LoadedRepository>>,
    graph: Option<Arc<RwLock<MultiplexGraph>>>,
    root_path: PathBuf,
    event_tx: Option<mpsc::Sender<WatcherEvent>>,
    debounce: Duration,
) {
    let mut pending_paths = HashSet::new();
    let canonical_root = std::fs::canonicalize(&root_path).ok();

    'outer: loop {
        if pending_paths.is_empty() {
            match rx.recv() {
                Ok(InternalMsg::Notify(Ok(event))) => {
                    for path in event.paths {
                        if !is_ignored_path(&path, &root_path)
                            && SupportedLanguage::from_path(&path).is_some()
                        {
                            pending_paths.insert(path);
                        }
                    }
                }
                Ok(InternalMsg::Notify(Err(_))) => {}
                Ok(InternalMsg::Stop) | Err(_) => break 'outer,
            }
        }

        while !pending_paths.is_empty() {
            match rx.recv_timeout(debounce) {
                Ok(InternalMsg::Notify(Ok(event))) => {
                    for path in event.paths {
                        if !is_ignored_path(&path, &root_path)
                            && SupportedLanguage::from_path(&path).is_some()
                        {
                            pending_paths.insert(path);
                        }
                    }
                }
                Ok(InternalMsg::Notify(Err(_))) => {}
                Ok(InternalMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break 'outer,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let mut any_changed = false;
                    let mut file_events = Vec::new();
                    {
                        let mut repo_lock = match repo.write() {
                            Ok(l) => l,
                            Err(p) => p.into_inner(),
                        };

                        for path in pending_paths.drain() {
                            let rel_opt = path.strip_prefix(&root_path).ok().or_else(|| {
                                canonical_root
                                    .as_ref()
                                    .and_then(|cr| path.strip_prefix(cr).ok())
                            });
                            let rel_path = rel_opt.unwrap_or(&path);
                            let is_deleted = !path.exists();
                            let changed = if is_deleted {
                                repo_lock.remove_file(rel_path).unwrap_or(false)
                            } else {
                                repo_lock.patch_file(rel_path).unwrap_or(false)
                            };

                            if changed {
                                any_changed = true;
                                let event_path = rel_opt
                                    .map(|p| p.to_path_buf())
                                    .unwrap_or_else(|| rel_path.to_path_buf());
                                if is_deleted {
                                    file_events
                                        .push(WatcherEvent::FileRemoved { path: event_path });
                                } else {
                                    file_events
                                        .push(WatcherEvent::FilePatched { path: event_path });
                                }
                            }
                        }
                    }

                    if any_changed {
                        let mut graph_event = None;
                        if let Some(ref graph_arc) = graph {
                            let repo_lock = match repo.read() {
                                Ok(l) => l,
                                Err(p) => p.into_inner(),
                            };
                            let new_graph = repo_lock.build_graph();
                            let num_symbols = new_graph.num_symbols();
                            let num_edges = new_graph.num_edges();
                            drop(repo_lock);

                            {
                                let mut graph_lock = match graph_arc.write() {
                                    Ok(l) => l,
                                    Err(p) => p.into_inner(),
                                };
                                *graph_lock = new_graph;
                            }

                            graph_event = Some(WatcherEvent::GraphUpdated {
                                num_symbols,
                                num_edges,
                            });
                        }

                        if let Some(ref tx) = event_tx {
                            for ev in file_events {
                                let _ = tx.send(ev);
                            }
                            if let Some(ev) = graph_event {
                                let _ = tx.send(ev);
                            }
                        }
                    }

                    break;
                }
            }
        }
    }
}
