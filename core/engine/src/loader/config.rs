use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::loader::{DynamicLoader, FilesystemLoader, FilesystemLoaderOptions, MemoryLoader};
use crate::model::DecisionContent;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LoaderConfig {
    Static {
        content: HashMap<String, DecisionContent>,
    },
    #[serde(rename = "fs")]
    Filesystem {
        path: String,
    },
    Zip {
        bytes: Vec<u8>,
    },
}

impl LoaderConfig {
    pub fn into_loader(self) -> anyhow::Result<DynamicLoader> {
        match self {
            LoaderConfig::Static { content } => {
                let loader = MemoryLoader::default();
                for (key, decision_content) in content {
                    loader.add(key, decision_content);
                }
                Ok(Arc::new(loader))
            }
            LoaderConfig::Filesystem { path } => {
                Ok(Arc::new(FilesystemLoader::new(FilesystemLoaderOptions {
                    root: path,
                })))
            }
            LoaderConfig::Zip { bytes } => Self::loader_from_zip(&bytes),
        }
    }

    fn loader_from_zip(bytes: &[u8]) -> anyhow::Result<DynamicLoader> {
        use std::io::Read;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let loader = MemoryLoader::default();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if !entry.is_file() {
                continue;
            }
            let name = entry.name()?;
            if !name.ends_with(".json") {
                continue;
            }

            let key = name.into_owned();
            let mut buffer = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buffer)?;
            let content: DecisionContent = serde_json::from_slice(&buffer)?;
            loader.add(key, content);
        }

        Ok(Arc::new(loader))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPH_JSON: &str = r#"{"nodes":[],"edges":[]}"#;

    #[tokio::test]
    async fn static_config_serves_decisions_by_key() {
        let mut content = HashMap::new();
        content.insert(
            "graph.json".to_string(),
            serde_json::from_str::<DecisionContent>(GRAPH_JSON).unwrap(),
        );

        let loader = LoaderConfig::Static { content }.into_loader().unwrap();
        assert!(loader.load("graph.json").await.is_ok());
        assert!(loader.load("missing.json").await.is_err());
    }

    #[test]
    fn fs_config_reads_path() {
        let config: LoaderConfig = serde_json::from_str(r#"{"type":"fs","path":"p"}"#).unwrap();

        let LoaderConfig::Filesystem { path } = config else {
            panic!("expected filesystem loader config");
        };

        assert_eq!(path, "p");
    }

    #[tokio::test]
    async fn zip_config_decodes_non_utf8_filenames() {
        use std::io::Write;

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("caf_.json", options).unwrap();
        writer.write_all(GRAPH_JSON.as_bytes()).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();

        // ZipWriter accepts UTF-8 names. Replace the ASCII placeholder in both
        // headers with CP437's é (invalid UTF-8), keeping lengths and data intact.
        let mut replaced = 0;
        for offset in 0..=bytes.len() - b"caf_.json".len() {
            if &bytes[offset..offset + b"caf_.json".len()] == b"caf_.json" {
                bytes[offset + 3] = 0x82;
                replaced += 1;
            }
        }
        assert_eq!(replaced, 2, "local and central filenames must match");

        // Establish that this is a readable archive with a non-UTF-8 name,
        // rather than a corrupt archive that fails before filename decoding.
        {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
            let entry = archive.by_index(0).unwrap();
            assert_eq!(entry.name_raw(), b"caf\x82.json");
            assert!(std::str::from_utf8(entry.name_raw()).is_err());
            assert_eq!(entry.name().unwrap(), "café.json");
        }

        let loader = LoaderConfig::Zip { bytes }.into_loader().unwrap();
        assert!(loader.load("café.json").await.is_ok());
        assert!(loader.load("caf_.json").await.is_err());
        assert!(loader.load("caf�.json").await.is_err());
    }

    #[tokio::test]
    async fn zip_config_decompresses_and_serves_decisions() {
        use std::io::Write;

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("graph.json", options).unwrap();
            writer.write_all(GRAPH_JSON.as_bytes()).unwrap();
            writer.add_directory("nested.json/", options).unwrap();
            writer.start_file("notes.txt", options).unwrap();
            writer.write_all(b"not a decision").unwrap();
            writer.start_file("nested/café.json", options).unwrap();
            writer.write_all(GRAPH_JSON.as_bytes()).unwrap();
            writer.finish().unwrap();
        }

        let loader = LoaderConfig::Zip {
            bytes: cursor.into_inner(),
        }
        .into_loader()
        .unwrap();

        assert!(loader.load("graph.json").await.is_ok());
        assert!(loader.load("nested/café.json").await.is_ok());
        assert!(loader.load("nested.json/").await.is_err());
        assert!(loader.load("notes.txt").await.is_err());
        assert!(loader.load("missing.json").await.is_err());
    }
}
