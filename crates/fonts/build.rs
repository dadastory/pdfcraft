//! Reads the optional craft-fonts input (`CRAFT_FONTS_DIR`) as `CRAFT_FONTS`.
//! A fonts/served.json input compiles metadata only; the host loads exact reviewed programs
//! on demand. The original manifest.txt input below retains its optional embedded behavior.
//! The recipe is craft-fonts' `docs/integration.md`; unset, `CRAFT_FONTS` is empty. It only reads
//! the local checkout (no network). On wasm32 it embeds only the UI faces (BIZ UDPGothic Regular
//! and any `Hans`, `Arab`, `Telu`, `Hebr`, `Thai` or `Deva` face), to keep the web build within hosting
//! limits (Cloudflare Pages: 25 MiB per file).

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    let mut entries = String::new();
    if let Some(dir) = std::env::var_os("CRAFT_FONTS_DIR").map(PathBuf::from) {
        match craft_fonts(&dir) {
            Ok(input) => entries.push_str(&input),
            Err(e) if std::env::var_os("CRAFT_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=CRAFT_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without craft-fonts: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    let count = entries.lines().filter(|line| line.trim_start().starts_with("CraftFont")).count();
    let src =
        format!("static CRAFT_FONT_STORAGE: [CraftFont; {count}] = [\n{entries}];\npub static CRAFT_FONTS: &[CraftFont] = &CRAFT_FONT_STORAGE;\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// Family-name prefixes of the faces AGENTS.md §1.1 forbids: Adobe's Source families and Noto
/// CJK, which is Source Han under another name. The test in `src/craft.rs` has the same list.
const BARRED_FAMILIES: [&str; 5] = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];

/// One `CraftFont { .. }` initialiser per manifest line.
fn craft_fonts(dir: &std::path::Path) -> Result<String, String> {
    let served = dir.join("fonts/served.json");
    println!("cargo::rerun-if-changed={}", served.display());
    if served.exists() {
        return served_fonts(&served);
    }
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        // AGENTS.md §1.1 bars Adobe's type designs whatever their licence, so a checkout that
        // carries one (craft-fonts has Noto Sans CJK, which is Source Han) never embeds it.
        if BARRED_FAMILIES.iter().any(|barred| family.starts_with(barred)) {
            println!("cargo::warning=craft-fonts: not embedding {family} {style} (AGENTS.md §1.1)");
            continue;
        }
        // Arabic interface faces are small, so the web build keeps them too.
        let arabic = scripts.split(',').any(|s| s.trim() == "Arab");
        // Telugu faces too, for the Telugu interface.
        let telugu = scripts.split(',').any(|s| s.trim() == "Telu");
        // And the Hebrew ones (small), for Hebrew text and Edit text on the web.
        let hebrew = scripts.split(',').any(|s| s.trim() == "Hebr");
        // Devanagari too, for the Hindi interface (one small face).
        let devanagari = scripts.split(',').any(|s| s.trim() == "Deva");
        let chinese = scripts.split(',').any(|s| s.trim() == "Hans");
        let web_face = (*family == "BIZ UDPGothic" && *style == "Regular")
            || chinese
            || arabic
            || telugu
            || hebrew
            || scripts.split(',').any(|s| s.trim() == "Thai")
            || devanagari;
        if wasm && !web_face {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    CraftFont::embedded({family:?}, {style:?}, &[{}], include_bytes!({:?})),",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}

/// Metadata only: the host serves the reviewed derivatives through its shared download cache.
/// Unlike the legacy embedded input, every script and style remains available on the web.
fn served_fonts(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let catalog: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let default = catalog["default"].as_str().ok_or("served catalog has no default")?;
    let entries = catalog["fonts"].as_array().ok_or("served catalog has no fonts")?;
    let mut identities = std::collections::HashSet::new();
    let mut out = String::new();
    for entry in entries {
        let field = |key: &str| entry[key].as_str().ok_or_else(|| format!("served font has no {key}"));
        let id = field("id")?;
        if id.is_empty() || !identities.insert(id) {
            return Err(format!("duplicate or empty served font identity: {id}"));
        }
        let family = field("family")?;
        let style = field("style")?;
        let label = format!("{family} {style}");
        let postscript = field("postscriptName")?;
        let url = field("url")?;
        if !url.starts_with("/fonts/") || url.contains(['?', '#']) {
            return Err(format!("served font URL must be a local versioned font asset: {url}"));
        }
        let digest = field("sha256")?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("served font has an invalid SHA-256: {id}"));
        }
        let hash: Vec<u8> = (0..64)
            .step_by(2)
            .map(|offset| u8::from_str_radix(digest.get(offset..offset + 2).unwrap_or_default(), 16).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let byte_size = entry["byteSize"].as_u64().filter(|n| *n > 0 && *n <= u64::from(u32::MAX)).ok_or("invalid font byte size")?;
        let local = field("localFile")?;
        let scripts: Vec<String> = entry["scripts"]
            .as_array()
            .ok_or("served font has no scripts")?
            .iter()
            .map(|s| s.as_str().map(|s| format!("{s:?}")).ok_or("invalid font script"))
            .collect::<Result<_, _>>()?;
        let _ = writeln!(
            out,
            "CraftFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: &[], served: Some(ServedFont {{ id: {id:?}, label: {label:?}, postscript_name: {postscript:?}, url: {url:?}, sha256: {hash:?}, byte_size: {byte_size}, default: {}, #[cfg(not(target_arch = \"wasm32\"))] local_file: {local:?}, program: OnceLock::new() }}) }},",
            scripts.join(","),
            id == default
        );
    }
    if !identities.contains(default) {
        return Err("served font default is absent from the catalog".into());
    }
    Ok(out)
}
