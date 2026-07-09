//! Model catalog + download/verify/delete (plan 1.4).
//!
//! Whisper GGMLs live flat in `<app-data>/models/` (so the Phase 0 small
//! install stays valid); Parakeet is a directory of ONNX files. Downloads
//! stream to a `.partial` file, SHA-256 verified where the source publishes
//! a hash (HF LFS), then renamed into place.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

pub const PARAKEET_DIR: &str = "parakeet-tdt-0.6b-v2";

pub struct FileSpec {
    pub url: &'static str,
    /// Path relative to the models dir.
    pub rel_path: &'static str,
    /// None for tiny non-LFS files (no published sha256); size is still checked.
    pub sha256: Option<&'static str>,
    pub size: u64,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub display: &'static str,
    pub engine: &'static str, // "whisper-cpp" | "parakeet"
    pub files: &'static [FileSpec],
    pub total_size: u64,
    pub speed_hint: &'static str,
    pub quality_hint: &'static str,
    pub english_only: bool,
}

pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "whisper-tiny",
        display: "Whisper tiny",
        engine: "whisper-cpp",
        files: &[FileSpec {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
            rel_path: "ggml-tiny.bin",
            sha256: Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"),
            size: 77_691_713,
        }],
        total_size: 77_691_713,
        speed_hint: "fastest whisper, ~1s for 10s speech",
        quality_hint: "rough — quick notes only",
        english_only: false,
    },
    ModelSpec {
        id: "whisper-base",
        display: "Whisper base",
        engine: "whisper-cpp",
        files: &[FileSpec {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            rel_path: "ggml-base.bin",
            sha256: Some("60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"),
            size: 147_951_465,
        }],
        total_size: 147_951_465,
        speed_hint: "very fast",
        quality_hint: "okay for clear speech",
        english_only: false,
    },
    ModelSpec {
        id: "whisper-small",
        display: "Whisper small",
        engine: "whisper-cpp",
        files: &[FileSpec {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            rel_path: "ggml-small.bin",
            sha256: Some("1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
            size: 487_601_967,
        }],
        total_size: 487_601_967,
        speed_hint: "~2s for 6s speech on M-series",
        quality_hint: "decent general dictation",
        english_only: false,
    },
    ModelSpec {
        id: "whisper-medium",
        display: "Whisper medium",
        engine: "whisper-cpp",
        files: &[FileSpec {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
            rel_path: "ggml-medium.bin",
            sha256: Some("6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208"),
            size: 1_533_763_059,
        }],
        total_size: 1_533_763_059,
        speed_hint: "slow — several seconds per utterance",
        quality_hint: "high",
        english_only: false,
    },
    ModelSpec {
        id: "whisper-large-v3-turbo",
        display: "Whisper large-v3-turbo",
        engine: "whisper-cpp",
        files: &[FileSpec {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
            rel_path: "ggml-large-v3-turbo.bin",
            sha256: Some("1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
            size: 1_624_555_275,
        }],
        total_size: 1_624_555_275,
        speed_hint: "moderate — best accuracy-per-second locally",
        quality_hint: "best local whisper",
        english_only: false,
    },
    ModelSpec {
        id: "parakeet-tdt-0.6b-v2",
        display: "Parakeet TDT 0.6B v2 (int8)",
        engine: "parakeet",
        files: &[
            FileSpec {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/encoder-model.int8.onnx",
                rel_path: "parakeet-tdt-0.6b-v2/encoder-model.int8.onnx",
                sha256: Some("3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155"),
                size: 652_184_014,
            },
            FileSpec {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/decoder_joint-model.int8.onnx",
                rel_path: "parakeet-tdt-0.6b-v2/decoder_joint-model.int8.onnx",
                sha256: Some("a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3"),
                size: 8_998_286,
            },
            FileSpec {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/nemo128.onnx",
                rel_path: "parakeet-tdt-0.6b-v2/nemo128.onnx",
                sha256: Some("a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f"),
                size: 139_764,
            },
            FileSpec {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/vocab.txt",
                rel_path: "parakeet-tdt-0.6b-v2/vocab.txt",
                sha256: None, // non-LFS file: HF publishes no sha256; size-checked
                size: 9_384,
            },
        ],
        total_size: 661_331_448,
        speed_hint: "fastest local engine — sub-second",
        quality_hint: "very good (English only)",
        english_only: true,
    },
];

pub fn models_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("resolving app data dir")?
        .join("models"))
}

pub fn whisper_model_path(models_dir: &Path, model_id: &str) -> Result<PathBuf> {
    let spec = CATALOG
        .iter()
        .find(|m| m.id == model_id && m.engine == "whisper-cpp")
        .with_context(|| format!("unknown whisper model '{model_id}'"))?;
    Ok(models_dir.join(spec.files[0].rel_path))
}

pub fn is_installed(models_dir: &Path, spec: &ModelSpec) -> bool {
    spec.files.iter().all(|f| {
        let p = models_dir.join(f.rel_path);
        p.exists() && fs::metadata(&p).map(|m| m.len() == f.size).unwrap_or(false)
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display: String,
    pub engine: String,
    pub total_size: u64,
    pub speed_hint: String,
    pub quality_hint: String,
    pub english_only: bool,
    pub installed: bool,
}

pub fn catalog_info(models_dir: &Path) -> Vec<ModelInfo> {
    CATALOG
        .iter()
        .map(|m| ModelInfo {
            id: m.id.into(),
            display: m.display.into(),
            engine: m.engine.into(),
            total_size: m.total_size,
            speed_hint: m.speed_hint.into(),
            quality_hint: m.quality_hint.into(),
            english_only: m.english_only,
            installed: is_installed(models_dir, m),
        })
        .collect()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub percent: u8,
    pub done: bool,
    pub error: Option<String>,
}

/// Blocking download of all files for a model, with `model://download` events.
pub fn download_model(app: &AppHandle, model_id: &str) -> Result<()> {
    let spec = CATALOG
        .iter()
        .find(|m| m.id == model_id)
        .with_context(|| format!("unknown model '{model_id}'"))?;
    let dir = models_dir(app)?;
    let mut fetched: u64 = 0;
    let mut last_pct = 255u8;
    for file in spec.files {
        let dest = dir.join(file.rel_path);
        if dest.exists() && fs::metadata(&dest)?.len() == file.size {
            fetched += file.size;
            continue;
        }
        fs::create_dir_all(dest.parent().expect("file has parent dir"))?;
        let tmp = dest.with_extension("partial");
        let mut res = ureq::get(file.url)
            .call()
            .with_context(|| format!("requesting {}", file.url))?;
        let mut reader = res.body_mut().as_reader();
        let mut out = fs::File::create(&tmp)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            fetched += n as u64;
            let pct = ((fetched * 100) / spec.total_size).min(100) as u8;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "model://download",
                    DownloadProgress {
                        model_id: model_id.into(),
                        percent: pct,
                        done: false,
                        error: None,
                    },
                );
            }
        }
        out.flush()?;
        drop(out);
        let digest = format!("{:x}", hasher.finalize());
        if let Some(expected) = file.sha256 {
            if digest != expected {
                let _ = fs::remove_file(&tmp);
                bail!("checksum mismatch for {} — download removed, retry", file.rel_path);
            }
        }
        let got = fs::metadata(&tmp)?.len();
        if got != file.size {
            let _ = fs::remove_file(&tmp);
            bail!("size mismatch for {} ({got} vs {})", file.rel_path, file.size);
        }
        fs::rename(&tmp, &dest)?;
        tracing::info!("downloaded {}", file.rel_path);
    }
    let _ = app.emit(
        "model://download",
        DownloadProgress {
            model_id: model_id.into(),
            percent: 100,
            done: true,
            error: None,
        },
    );
    Ok(())
}

pub fn delete_model(app: &AppHandle, model_id: &str) -> Result<()> {
    let spec = CATALOG
        .iter()
        .find(|m| m.id == model_id)
        .with_context(|| format!("unknown model '{model_id}'"))?;
    let dir = models_dir(app)?;
    for file in spec.files {
        let p = dir.join(file.rel_path);
        if p.exists() {
            fs::remove_file(&p)?;
        }
    }
    // Remove the parakeet subdirectory if now empty.
    if spec.engine == "parakeet" {
        let _ = fs::remove_dir(dir.join(PARAKEET_DIR));
    }
    Ok(())
}
