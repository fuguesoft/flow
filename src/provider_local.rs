use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    model::Board,
    provider::{MoveMode, Provider, ProviderError},
    store_fs,
};

const DEMO_BOARD: &[(&str, &str)] = &[
    ("board.txt", include_str!("../boards/demo/board.txt")),
    (
        "cols/todo/order.txt",
        include_str!("../boards/demo/cols/todo/order.txt"),
    ),
    (
        "cols/todo/FLOW-1.md",
        include_str!("../boards/demo/cols/todo/FLOW-1.md"),
    ),
    (
        "cols/todo/FLOW-2.md",
        include_str!("../boards/demo/cols/todo/FLOW-2.md"),
    ),
    (
        "cols/in_progress/order.txt",
        include_str!("../boards/demo/cols/in_progress/order.txt"),
    ),
    (
        "cols/in_progress/FLOW-3.md",
        include_str!("../boards/demo/cols/in_progress/FLOW-3.md"),
    ),
    (
        "cols/in_progress/FLOW-4.md",
        include_str!("../boards/demo/cols/in_progress/FLOW-4.md"),
    ),
    (
        "cols/in_review/order.txt",
        include_str!("../boards/demo/cols/in_review/order.txt"),
    ),
    (
        "cols/in_review/FLOW-5.md",
        include_str!("../boards/demo/cols/in_review/FLOW-5.md"),
    ),
    (
        "cols/done/order.txt",
        include_str!("../boards/demo/cols/done/order.txt"),
    ),
    (
        "cols/done/FLOW-6.md",
        include_str!("../boards/demo/cols/done/FLOW-6.md"),
    ),
];

pub struct LocalProvider {
    root: PathBuf,
}

impl LocalProvider {
    pub fn from_env() -> Self {
        if let Ok(p) = std::env::var("FLOW_BOARD_PATH") {
            return Self {
                root: PathBuf::from(p),
            };
        }

        if std::env::var("FLOW_PROVIDER").ok().as_deref() == Some("local") {
            if let Ok(p) = std::env::var("FLOW_LOCAL_PATH") {
                return Self {
                    root: PathBuf::from(p),
                };
            }
            if let Ok(home) = std::env::var("HOME") {
                return Self {
                    root: PathBuf::from(home).join(".config/flow/boards/default"),
                };
            }
        }

        Self { root: demo_root() }
    }
}

fn demo_root() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("boards/demo");
    if source.exists() {
        return source;
    }

    let root = std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/flow/boards/demo"))
        .unwrap_or_else(|_| std::env::temp_dir().join("flow/boards/demo"));
    if !root.exists() {
        let _ = seed_demo(&root);
    }
    root
}

fn seed_demo(root: &Path) -> io::Result<()> {
    for (rel, contents) in DEMO_BOARD {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        fs::write(path, contents)?;
    }
    Ok(())
}

impl Provider for LocalProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        store_fs::load_board(&self.root).map_err(|e| map_load_err("load_board", &self.root, e))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        store_fs::move_card(&self.root, card_id, to_col_id)
            .map_err(|e| map_move_err(card_id, &self.root, e))
    }

    fn move_mode(&self) -> MoveMode {
        MoveMode::Sync
    }

    fn create_card(&mut self, to_col_id: &str) -> Result<String, ProviderError> {
        store_fs::create_card(&self.root, to_col_id).map_err(|err| ProviderError::Io {
            op: "create_card".to_string(),
            path: self.root.clone(),
            source: err,
        })
    }

    fn card_path(&self, card_id: &str) -> Result<PathBuf, ProviderError> {
        store_fs::card_path(&self.root, card_id).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => ProviderError::NotFound {
                id: card_id.to_string(),
            },
            _ => ProviderError::Io {
                op: "card_path".to_string(),
                path: self.root.clone(),
                source: err,
            },
        })
    }
}

fn map_load_err(op: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: op.to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

fn map_move_err(card_id: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::NotFound => ProviderError::NotFound {
            id: card_id.to_string(),
        },
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: "move_card".to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn tmp_root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-provider-test-{n}"))
    }

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn map_load_err_returns_parse_for_invalid_data() {
        let root = PathBuf::from("/tmp/flow-test");
        let err = map_load_err(
            "load_board",
            &root,
            io::Error::new(io::ErrorKind::InvalidData, "bad"),
        );

        assert!(matches!(err, ProviderError::Parse { .. }));
    }

    #[test]
    fn move_card_returns_not_found() {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n");

        let mut provider = LocalProvider { root: root.clone() };
        let err = provider.move_card("X-1", "todo").unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "X-1"),
            _ => panic!("expected NotFound error"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn seed_demo_writes_loadable_board() {
        let root = tmp_root();
        seed_demo(&root).unwrap();

        let board = store_fs::load_board(&root).unwrap();
        let cards: usize = board.columns.iter().map(|c| c.cards.len()).sum();
        assert_eq!(cards, 6);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn demo_board_embeds_every_source_file() {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    out.push(path.strip_prefix(root).unwrap().to_string_lossy().into());
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("boards/demo");
        let mut on_disk = Vec::new();
        walk(&root, &root, &mut on_disk);
        on_disk.sort();

        let mut embedded: Vec<String> = DEMO_BOARD.iter().map(|(rel, _)| rel.to_string()).collect();
        embedded.sort();

        assert_eq!(on_disk, embedded);
    }
}
