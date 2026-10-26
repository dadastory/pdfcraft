//! Fonts from the optional craft-fonts build input (<https://github.com/storytold/craft-fonts>).
//!
//! `build.rs` embeds every font in craft-fonts' manifest when PdfCraft is built with
//! `CRAFT_FONTS_DIR=<checkout>`; otherwise [`CRAFT_FONTS`] is empty and everything here returns
//! nothing. Callers must work either way.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

static GENERATION: AtomicUsize = AtomicUsize::new(0);

pub fn font_generation() -> usize {
    GENERATION.load(Ordering::Relaxed)
}

/// A reviewed static program served by the host instead of embedded in the application.
pub struct ServedFont {
    pub id: &'static str,
    pub label: &'static str,
    pub postscript_name: &'static str,
    pub url: &'static str,
    pub sha256: [u8; 32],
    pub byte_size: usize,
    pub default: bool,
    /// Build input location, used only by native acceptance tests, never a network fallback.
    #[cfg(not(target_arch = "wasm32"))]
    pub local_file: &'static str,
    program: OnceLock<Vec<u8>>,
}

/// A font from the optional craft-fonts build input (empty unless built with `CRAFT_FONTS_DIR`).
pub struct CraftFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Jpan"`.
    pub scripts: &'static [&'static str],
    pub bytes: &'static [u8],
    pub served: Option<ServedFont>,
}

include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));

impl CraftFont {
    pub const fn embedded(family: &'static str, style: &'static str, scripts: &'static [&'static str], bytes: &'static [u8]) -> Self {
        Self { family, style, scripts, bytes, served: None }
    }

    /// Immutable after registration; empty while a served face has not been requested.
    pub fn data(&self) -> &[u8] {
        self.served.as_ref().and_then(|source| source.program.get()).map_or(self.bytes, Vec::as_slice)
    }

    pub fn is_loaded(&self) -> bool {
        !self.data().is_empty()
    }

    pub fn key(&self) -> String {
        self.served.as_ref().map_or_else(|| self.name(), |source| source.id.to_owned())
    }

    pub fn label(&self) -> &str {
        self.served.as_ref().map_or(self.family, |source| source.label)
    }

    /// Admit only the exact static derivative pinned in the build catalog. Uploads cannot
    /// acquire this authority by choosing a matching family or PDF resource name.
    pub fn register(&self, bytes: Vec<u8>) -> Result<(), String> {
        let source = self.served.as_ref().ok_or("this font is embedded in the application")?;
        source.verify(&bytes)?;
        crate::embedding::validate_packaged_structure(&bytes)?;
        if source.program.set(bytes).is_ok() {
            GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
    /// Whether the font is meant for `script` (ISO 15924, e.g. `"Jpan"`).
    pub fn covers(&self, script: &str) -> bool {
        self.scripts.contains(&script)
    }

    /// A unique name for registering the face with a font system ("BIZ UDPGothic Bold").
    pub fn name(&self) -> String {
        format!("{} {}", self.family, self.style)
    }
}

impl ServedFont {
    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        if bytes.len() != self.byte_size || Sha256::digest(bytes).as_slice() != self.sha256 {
            return Err("the font bytes do not match the reviewed catalog; retry loading the font".into());
        }
        Ok(())
    }
}

/// The `Jpan` craft-fonts faces in the order the interface prefers them: BIZ UDPGothic (a UI
/// face) first, then the rest in manifest order. Empty without craft-fonts.
pub fn ui_japanese_fonts() -> Vec<&'static CraftFont> {
    let mut fonts: Vec<&CraftFont> = CRAFT_FONTS.iter().filter(|f| f.covers("Jpan")).collect();
    // Stable: manifest order within each group.
    fonts.sort_by_key(|f| f.family != "BIZ UDPGothic");
    fonts
}

/// The `Hans` craft-fonts faces for Simplified Chinese interface text, in manifest order.
/// Empty when built without craft-fonts (Chinese text then shows the font system's
/// replacement glyph, the same degraded mode as Japanese without craft-fonts).
pub fn ui_chinese_fonts() -> Vec<&'static CraftFont> {
    CRAFT_FONTS.iter().filter(|f| f.covers("Hans")).collect()
}

/// The `Arab` craft-fonts faces for Arabic-script interface text (file names, document titles),
/// in manifest order. Empty when built without craft-fonts or when it has no Arabic face.
pub fn ui_arabic_fonts() -> Vec<&'static CraftFont> {
    arabic(CRAFT_FONTS.iter())
}

/// The face for Arabic text written into PDFs: the first `Arab` face. `None` when built without
/// craft-fonts or when its revision has no Arabic face.
pub fn document_arabic_font() -> Option<&'static CraftFont> {
    ui_arabic_fonts().into_iter().find(|face| face.is_loaded())
}

fn arabic<'a>(faces: impl IntoIterator<Item = &'a CraftFont>) -> Vec<&'a CraftFont> {
    faces.into_iter().filter(|f| f.covers("Arab")).collect()
}

/// The `Telu` craft-fonts faces for Telugu interface text (the Telugu catalog, file names,
/// document titles), in manifest order. Empty when built without craft-fonts or when it has no
/// Telugu face.
pub fn ui_telugu_fonts() -> Vec<&'static CraftFont> {
    CRAFT_FONTS.iter().filter(|f| f.covers("Telu")).collect()
}

/// Thai faces shared by the interface and PDF text writer.
pub fn ui_thai_fonts() -> Vec<&'static CraftFont> {
    CRAFT_FONTS.iter().filter(|f| f.covers("Thai")).collect()
}

pub fn document_thai_font() -> Option<&'static CraftFont> {
    ui_thai_fonts().into_iter().find(|face| face.is_loaded())
}

/// The `Deva` craft-fonts faces for Devanagari interface text (the Hindi catalog, file names,
/// document titles), in manifest order. Empty when built without craft-fonts or when its revision
/// has no Devanagari face; a desktop build then falls back to one font already installed on the
/// machine (`SYSTEM_DEVANAGARI_FALLBACK`), and the web build shows replacement boxes.
pub fn ui_devanagari_fonts() -> Vec<&'static CraftFont> {
    CRAFT_FONTS.iter().filter(|f| f.covers("Deva")).collect()
}

/// The face for Devanagari text written into PDFs: the first `Deva` face. `None` when built
/// without craft-fonts or when its revision has no Devanagari face.
pub fn document_devanagari_font() -> Option<&'static CraftFont> {
    ui_devanagari_fonts().into_iter().find(|face| face.is_loaded())
}

/// The `Hebr` craft-fonts faces for Hebrew interface text (file names, document titles, text
/// being edited), in manifest order. Empty when built without craft-fonts or when it has no
/// Hebrew face.
pub fn ui_hebrew_fonts() -> Vec<&'static CraftFont> {
    CRAFT_FONTS.iter().filter(|f| f.covers("Hebr")).collect()
}

/// The face for Hebrew text written into PDFs: the `Hebr` face of the asked weight, else its
/// Regular, else any `Hebr` face. `None` without craft-fonts or without a Hebrew face.
pub fn document_hebrew_font(bold: bool) -> Option<&'static CraftFont> {
    hebrew_face(CRAFT_FONTS, bold)
}

fn hebrew_face(faces: &[CraftFont], bold: bool) -> Option<&CraftFont> {
    let hebr = || faces.iter().filter(|f| f.covers("Hebr") && f.is_loaded());
    let style = if bold { "Bold" } else { "Regular" };
    hebr().find(|f| f.style == style).or_else(|| hebr().find(|f| f.style == "Regular")).or_else(|| hebr().next())
}

/// Interface CJK faces in fallback order for the UI language: Simplified Chinese first when
/// `prefer_hans`, otherwise Japanese first (the historical default).
///
/// The order matters beyond glyph shapes. egui renders each character with the FIRST face that
/// has its glyph, so with Japanese first a Simplified-only character (e.g. U+6B22 欢, absent
/// from Japanese faces) lands in a different face than its neighbours; the mixed vertical
/// metrics then sink it below the line. Preferring the UI language's face keeps one line in
/// one face with one baseline.
pub fn ui_cjk_fonts(prefer_hans: bool) -> Vec<&'static CraftFont> {
    order_cjk(CRAFT_FONTS.iter(), prefer_hans)
}

fn order_cjk<'a>(faces: impl IntoIterator<Item = &'a CraftFont>, prefer_hans: bool) -> Vec<&'a CraftFont> {
    let mut out: Vec<&'a CraftFont> = faces.into_iter().filter(|f| f.covers("Hans") || f.covers("Jpan")).collect();
    let first = if prefer_hans { "Hans" } else { "Jpan" };
    // The preferred script's faces first (a face covering both counts as preferred); within each
    // group the Japanese UI face (BIZ UDPGothic, not a serif) leads, otherwise manifest order
    // (the sort is stable).
    out.sort_by_key(|f| (!f.covers(first), f.family != "BIZ UDPGothic"));
    out
}

/// The face for Japanese text written into PDFs (serif document text): Shippori Mincho, then
/// BIZ UDMincho, then any other regular `Jpan` face. `None` without craft-fonts.
pub fn document_japanese_font() -> Option<&'static CraftFont> {
    document_face(CRAFT_FONTS, true, false)
}

/// A real Japanese document face matching serif/sans and weight where available.
/// Sans text prefers BIZ UDPGothic Bold or Regular. Missing weights fall back to Regular;
/// serif text keeps the document Mincho preference. No synthetic weight or slant is applied.
/// `None` without craft-fonts. The small web input currently has only Gothic Regular.
pub fn document_japanese_font_for_style(serif: bool, bold: bool) -> Option<&'static CraftFont> {
    document_face(CRAFT_FONTS, serif, bold)
}

/// Every `Jpan` face that can stand in for the document text, the best match for the style first
/// (the face [`document_japanese_font_for_style`] returns), then the others. Not every face has
/// every glyph (their coverage of e.g. Cyrillic differs), so a caller can move on to the next one.
/// Empty without craft-fonts.
pub fn document_japanese_fonts_for_style(serif: bool, bold: bool) -> Vec<&'static CraftFont> {
    document_faces(CRAFT_FONTS, serif, bold)
}

/// The scripts the bundled Inter faces are for.
static LATIN_SCRIPTS: &[&str] = &["Latn", "Cyrl", "Grek"];

/// The bundled Latin document faces (`assets/fonts`, OFL): every build has them, unlike
/// craft-fonts. The Japanese faces carry only part of the Cyrillic (`привет` yes, Uzbek `қизил`
/// and Kazakh `әже` no), so letters like қ, ғ, ҳ, ә, ө, ү, ұ and һ have no face at all without
/// these.
static INTER_REGULAR_FACE: CraftFont =
    CraftFont::embedded("Inter", "Regular", LATIN_SCRIPTS, include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
static INTER_SEMIBOLD_FACE: CraftFont =
    CraftFont::embedded("Inter", "SemiBold", LATIN_SCRIPTS, include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));

/// The face for Latin, Greek and Cyrillic text written into PDFs: the bundled Inter Regular, or
/// SemiBold for bold text. Present in every build, crafted ones included.
pub fn document_latin_fonts(bold: bool) -> Vec<&'static CraftFont> {
    vec![if bold { &INTER_SEMIBOLD_FACE } else { &INTER_REGULAR_FACE }]
}

fn document_face(faces: &[CraftFont], serif: bool, bold: bool) -> Option<&CraftFont> {
    document_faces(faces, serif, bold).into_iter().next()
}

fn document_faces(faces: &[CraftFont], serif: bool, bold: bool) -> Vec<&CraftFont> {
    let jpan = || faces.iter().filter(|f| f.covers("Jpan") && f.is_loaded());
    let mut preferred = Vec::new();
    if !serif {
        let style = if bold { "Bold" } else { "Regular" };
        preferred.extend(jpan().find(|f| f.family == "BIZ UDPGothic" && f.style == style));
        preferred.extend(jpan().find(|f| f.family == "BIZ UDPGothic" && f.style == "Regular"));
    }
    for family in ["Shippori Mincho", "BIZ UDMincho"] {
        preferred.extend(jpan().find(|f| f.family == family && f.style == "Regular"));
    }
    preferred.extend(jpan().find(|face| face.served.as_ref().is_some_and(|source| source.default)));
    let mut out: Vec<&CraftFont> = Vec::new();
    for face in preferred.into_iter().chain(jpan().filter(|f| f.style == "Regular")).chain(jpan()) {
        if !out.iter().any(|f| std::ptr::eq(*f, face)) {
            out.push(face);
        }
    }
    out
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn find(family: &str, style: &str) -> Option<&'static [u8]> {
    let mut i = 0;
    while i < CRAFT_FONTS.len() {
        let f = &CRAFT_FONTS[i];
        if str_eq(f.family, family) && str_eq(f.style, style) {
            return Some(f.bytes);
        }
        i += 1;
    }
    None
}

/// Shippori Mincho Regular from craft-fonts, the preferred face for Japanese document text.
/// `None` when PdfCraft was built without craft-fonts.
pub static SHIPPORI_MINCHO: Option<&[u8]> = find("Shippori Mincho", "Regular");

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn served_default_is_a_ready_document_fallback_without_loading_the_catalog() {
        let Some(default) = CRAFT_FONTS.iter().find(|face| face.served.as_ref().is_some_and(|source| source.default)) else { return };
        let source = default.served.as_ref().unwrap();
        default.register(std::fs::read(source.local_file).unwrap()).unwrap();
        assert!(std::ptr::eq(document_japanese_font().unwrap(), default));
        assert!(crate::japanese_glyph('中').is_ok(), "the startup face can write Chinese without choosing another catalog font");
        if let Some(other) = CRAFT_FONTS.iter().find(|face| face.covers("Jpan") && !std::ptr::eq(*face, default) && face.served.is_some()) {
            other.register(std::fs::read(other.served.as_ref().unwrap().local_file).unwrap()).unwrap();
            assert!(std::ptr::eq(document_japanese_font().unwrap(), default), "loading another face must not change the default fallback");
        }
    }

    #[test]
    fn served_registration_verifies_bytes_before_reusing_one_program() {
        use sha2::{Digest, Sha256};
        let original = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let face = CraftFont {
            family: "Inter",
            style: "Regular",
            scripts: &["Latn"],
            bytes: &[],
            served: Some(ServedFont {
                id: "inter-regular",
                label: "Inter Regular",
                postscript_name: "Inter-Regular",
                url: "/fonts/test/inter.ttf",
                sha256: Sha256::digest(original).into(),
                byte_size: original.len(),
                default: true,
                #[cfg(not(target_arch = "wasm32"))]
                local_file: "unused",
                program: OnceLock::new(),
            }),
        };
        assert!(!face.is_loaded());
        let mut wrong = original.to_vec();
        wrong[0] ^= 1;
        assert!(face.register(wrong).is_err());
        assert!(!face.is_loaded(), "a failed download must remain retryable");
        face.register(original.to_vec()).unwrap();
        let address = face.data().as_ptr();
        face.register(original.to_vec()).unwrap();
        assert_eq!(address, face.data().as_ptr(), "another window reuses the immutable program");
        assert!(face.data() == original, "the registered bytes must remain exact");
        let mut malformed = original.to_vec();
        malformed[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
        let broken = CraftFont {
            served: Some(ServedFont {
                id: "broken",
                label: "Broken",
                postscript_name: "Broken",
                url: "/fonts/test/broken.ttf",
                sha256: Sha256::digest(&malformed).into(),
                byte_size: malformed.len(),
                default: false,
                #[cfg(not(target_arch = "wasm32"))]
                local_file: "unused",
                program: OnceLock::new(),
            }),
            ..CraftFont::embedded("Broken", "Regular", &["Latn"], &[])
        };
        assert!(broken.register(malformed).is_err(), "a correct digest does not bypass structural bounds");
        assert!(!broken.is_loaded());
    }

    #[test]
    fn cjk_fallback_order_follows_the_ui_language() {
        // Synthetic faces: order_cjk must not depend on the real build input.
        static BYTES: &[u8] = b"fake";
        static LATN: &[&str] = &["Latn"];
        static JPAN_LATN: &[&str] = &["Jpan", "Latn"];
        static HANS_LATN: &[&str] = &["Hans", "Latn"];
        let biz_bold = CraftFont::embedded("BIZ UDPGothic", "Bold", JPAN_LATN, BYTES);
        let biz = CraftFont::embedded("BIZ UDPGothic", "Regular", JPAN_LATN, BYTES);
        let hans = CraftFont::embedded("FakeHans", "Regular", HANS_LATN, BYTES);
        let inter = CraftFont::embedded("Inter", "Regular", LATN, BYTES);
        let faces = [biz_bold, biz, hans, inter];
        // Chinese mode: the Hans group first (manifest order), then Japanese.
        let zh: Vec<&str> = order_cjk([&faces[0], &faces[2], &faces[1]], true).iter().map(|f| f.family).collect();
        assert_eq!(zh, ["FakeHans", "BIZ UDPGothic", "BIZ UDPGothic"]);
        // Japanese mode keeps the historical default: the Japanese UI face first.
        let ja: Vec<&str> = order_cjk([&faces[0], &faces[2], &faces[1]], false).iter().map(|f| f.family).collect();
        assert_eq!(ja, ["BIZ UDPGothic", "BIZ UDPGothic", "FakeHans"]);
        // Latin-only faces are never interface CJK fallbacks.
        assert!(order_cjk(&faces, true).iter().all(|f| f.covers("Hans") || f.covers("Jpan")));
        // A serif listed first in the manifest never leads the Japanese group, in either mode.
        let mincho = CraftFont::embedded("Shippori Mincho", "Regular", JPAN_LATN, BYTES);
        let zh: Vec<&str> = order_cjk([&mincho, &faces[2], &faces[1]], true).iter().map(|f| f.family).collect();
        assert_eq!(zh, ["FakeHans", "BIZ UDPGothic", "Shippori Mincho"]);
        let ja: Vec<&str> = order_cjk([&mincho, &faces[2], &faces[1]], false).iter().map(|f| f.family).collect();
        assert_eq!(ja, ["BIZ UDPGothic", "Shippori Mincho", "FakeHans"]);
    }

    #[test]
    fn document_faces_match_style_without_inventing_missing_weights() {
        let faces = [
            CraftFont::embedded("Shippori Mincho", "Regular", &["Jpan"], b"serif"),
            CraftFont::embedded("BIZ UDPGothic", "Regular", &["Jpan"], b"sans"),
            CraftFont::embedded("BIZ UDPGothic", "Bold", &["Jpan"], b"bold"),
            CraftFont::embedded("BIZ UDPGothic", "Bold", &["Latn"], b"not-japanese"),
        ];
        assert_eq!(document_face(&faces, false, false).unwrap().bytes, b"sans");
        assert_eq!(document_face(&faces, false, true).unwrap().bytes, b"bold");
        assert_eq!(document_face(&faces, true, false).unwrap().bytes, b"serif");
        assert_eq!(document_face(&faces, true, true).unwrap().bytes, b"serif");
        assert_eq!(document_face(&faces[..2], false, true).unwrap().bytes, b"sans");
        assert_eq!(document_face(&faces[..1], false, true).unwrap().bytes, b"serif");
        // Every Japanese face is a candidate, the style's match first and none twice.
        let order: Vec<&[u8]> = document_faces(&faces, false, true).iter().map(|f| f.bytes).collect();
        assert_eq!(order, [b"bold".as_slice(), b"sans", b"serif"]);
        let order: Vec<&[u8]> = document_faces(&faces, true, false).iter().map(|f| f.bytes).collect();
        assert_eq!(order, [b"serif".as_slice(), b"sans", b"bold"]);
        assert!(document_face(&faces[3..], false, true).is_none());
        assert!(document_face(&[], false, true).is_none());
    }

    #[test]
    fn arabic_faces_are_picked_by_script_in_manifest_order() {
        // Synthetic faces: the filter must not depend on the real build input.
        static BYTES: &[u8] = b"fake";
        static ARAB_LATN: &[&str] = &["Arab", "Latn"];
        static JPAN: &[&str] = &["Jpan"];
        let naskh = CraftFont::embedded("FakeNaskh", "Regular", ARAB_LATN, BYTES);
        let biz = CraftFont::embedded("BIZ UDPGothic", "Regular", JPAN, BYTES);
        let kufi = CraftFont::embedded("FakeKufi", "Regular", ARAB_LATN, BYTES);
        let faces = [naskh, biz, kufi];
        let ar: Vec<&str> = arabic(&faces).iter().map(|f| f.family).collect();
        assert_eq!(ar, ["FakeNaskh", "FakeKufi"]);
        // An Arabic face is never a CJK fallback, and the other way round.
        assert!(order_cjk(&faces, false).iter().all(|f| f.family == "BIZ UDPGothic"));
        assert_eq!(ui_arabic_fonts().len(), CRAFT_FONTS.iter().filter(|f| f.covers("Arab")).count());
    }

    /// Family-name prefixes `build.rs` never embeds (its own `BARRED_FAMILIES`): Adobe's type
    /// designs, which includes Noto CJK (Source Han under another name).
    const BARRED_FAMILIES: [&str; 5] = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];

    /// Whatever the craft-fonts checkout holds, no Adobe type design is embedded (AGENTS.md §1.1).
    #[test]
    fn barred_families_are_never_embedded() {
        for face in CRAFT_FONTS {
            assert!(!BARRED_FAMILIES.iter().any(|barred| face.family.starts_with(barred)), "{} is embedded", face.name());
        }
        assert!(BARRED_FAMILIES.iter().any(|barred| "Noto Sans CJK SC".starts_with(barred)));
        assert!(!BARRED_FAMILIES.iter().any(|barred| "Noto Sans Arabic".starts_with(barred)));
    }

    #[test]
    fn hebrew_document_faces_follow_the_weight() {
        static HEBR_LATN: &[&str] = &["Hebr", "Latn"];
        let faces = [
            CraftFont::embedded("BIZ UDPGothic", "Bold", &["Jpan"], b"not-hebrew"),
            CraftFont::embedded("FakeHebrew", "Regular", HEBR_LATN, b"regular"),
            CraftFont::embedded("FakeHebrew", "Bold", HEBR_LATN, b"bold"),
        ];
        assert_eq!(hebrew_face(&faces, false).map(|f| f.bytes), Some(&b"regular"[..]));
        assert_eq!(hebrew_face(&faces, true).map(|f| f.bytes), Some(&b"bold"[..]));
        // No bold face: Regular, never a face of another script.
        assert_eq!(hebrew_face(&faces[..2], true).map(|f| f.bytes), Some(&b"regular"[..]));
        assert!(hebrew_face(&faces[..1], false).is_none());
        assert_eq!(ui_hebrew_fonts().len(), CRAFT_FONTS.iter().filter(|f| f.covers("Hebr")).count());
        assert_eq!(document_hebrew_font(false).is_some(), CRAFT_FONTS.iter().any(|f| f.covers("Hebr") && f.is_loaded()));
    }

    /// The bundled Inter faces carry the Latin, Greek and Cyrillic letters the craft-fonts faces
    /// lack: Uzbek қ ғ ҳ, Kazakh ә ө ү ұ һ, Ukrainian ї є ґ, Greek. This one runs with or without
    /// CRAFT_FONTS_DIR (#2593).
    #[test]
    fn latin_faces_carry_the_letters_the_japanese_faces_lack() {
        for face in document_latin_fonts(false).into_iter().chain(document_latin_fonts(true)) {
            for ch in ['қ', 'ғ', 'ҳ', 'ә', 'ө', 'ү', 'ұ', 'һ', 'ї', 'є', 'ґ', 'ё', 'α', 'β', 'γ', '≥', '≈'] {
                assert!(crate::japanese_glyph_from(face, ch).is_ok(), "{} has no {ch}", face.name());
            }
        }
        assert_eq!(document_latin_fonts(false)[0].style, "Regular");
        assert_eq!(document_latin_fonts(true)[0].style, "SemiBold");
    }

    #[test]
    fn craft_fonts_are_optional_and_consistent() {
        // Holds with or without CRAFT_FONTS_DIR.
        assert_eq!(SHIPPORI_MINCHO.is_some(), CRAFT_FONTS.iter().any(|f| f.family == "Shippori Mincho" && f.style == "Regular"));
        assert_eq!(document_japanese_font().is_some(), CRAFT_FONTS.iter().any(|f| f.covers("Jpan") && f.is_loaded()));
        let ui = ui_japanese_fonts();
        assert_eq!(ui.len(), CRAFT_FONTS.iter().filter(|f| f.covers("Jpan")).count());
        let ui_zh = ui_chinese_fonts();
        assert_eq!(ui_zh.len(), CRAFT_FONTS.iter().filter(|f| f.covers("Hans")).count());
        if CRAFT_FONTS.is_empty() {
            eprintln!("built without craft-fonts (CRAFT_FONTS_DIR unset): no Japanese faces, as expected");
            assert!(SHIPPORI_MINCHO.is_none() && ui.is_empty() && ui_zh.is_empty());
            return;
        }
        assert!(CRAFT_FONTS.iter().all(|f| !f.bytes.is_empty() || f.served.as_ref().is_some_and(|source| source.byte_size > 0)));
        if CRAFT_FONTS.iter().any(|f| f.family == "BIZ UDPGothic") {
            assert_eq!(ui.first().map(|f| f.family), Some("BIZ UDPGothic"));
        }
        if CRAFT_FONTS.iter().any(|face| face.family == "Shippori Mincho" && face.style == "Regular") {
            assert_eq!(document_japanese_font().map(|f| f.family), Some("Shippori Mincho"));
        }
    }
}
