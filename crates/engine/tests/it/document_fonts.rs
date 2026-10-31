//! Document font selection and contextual glyph save/reopen acceptance on synthetic PDFs.
use crate::support;
use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream};
use pdfcraft_engine::{BlockStyle, Edit, Session};
use pdfcraft_render::{PageRenderer, RenderRequest};
use std::sync::Arc;
fn document() -> Arc<Vec<u8>> {
    let mut d = Document::new_empty();
    let pages = d.add(Object::Dict(Dict::new()));
    let contents = d.add(Object::Stream(Stream::flate(Dict::new(), b"BT /F1 18 Tf 30 160 Td (Source) Tj ET")));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(font));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    let mut p = Dict::new();
    p.set(b"Type".to_vec(), Object::name("Page"));
    p.set(b"Parent".to_vec(), Object::Ref(pages));
    p.set(b"Contents".to_vec(), Object::Ref(contents));
    p.set(b"Resources".to_vec(), Object::Dict(res));
    p.set(b"MediaBox".to_vec(), Object::Array([0, 0, 300, 240].map(Object::Int).to_vec()));
    let page = d.add(Object::Dict(p));
    d.update_dict(pages, |p| {
        p.set(b"Type".to_vec(), Object::name("Pages"));
        p.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page)]));
        p.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    let mut root = Dict::new();
    root.set(b"Type".to_vec(), Object::name("Catalog"));
    root.set(b"Pages".to_vec(), Object::Ref(pages));
    let root = d.add(Object::Dict(root));
    d.trailer_mut().set(b"Root".to_vec(), Object::Ref(root));
    Arc::new(pdfcraft_cos::write_full(&d, &SaveOptions::default()).unwrap())
}
fn pixels(bytes: Arc<Vec<u8>>) -> Vec<u8> {
    let mut renderer = PageRenderer::new(bytes, Default::default());
    let p = renderer.render(RenderRequest { page: 0, scale: 2.0, ..Default::default() });
    assert!(p.error.is_none(), "{:?}", p.error);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    p.rgba.to_vec()
}
fn exercise(font: Vec<u8>, text: &str, expected_family: &str) {
    let mut s = Session::new();
    let id = s.open("source.pdf", None, document(), None).unwrap();
    let original = pixels(s.get(id).unwrap().display.clone());
    s.apply(id, Edit::RegisterTextFont { page: 0, bytes: Arc::new(font.clone()) }).unwrap();
    let resource = s.get(id).unwrap().text_editor_fonts(0).into_iter().find(|f| f.bytes == font).unwrap().resource;
    let style = BlockStyle { resource: Some(resource), width: Some(240.0), ..Default::default() };
    s.apply(id, Edit::EditTextBlock { page: 0, block: 0, text: text.into(), style: style.clone() }).unwrap();
    let edited = s.get(id).unwrap();
    let lines = edited.text_lines(0);
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), text);
    assert!(lines.iter().all(|l| l.base_font == expected_family));
    assert!((lines[0].baseline()[1] - 160.0).abs() < 0.01);
    let current = pixels(edited.display.clone());
    assert_ne!(current, original);
    let saved = s.save_bytes(id).unwrap();
    let re = s.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(pixels(s.get(re).unwrap().display.clone()), current);
    assert_eq!(s.get(re).unwrap().text_blocks(0)[0].editor_text, text);
    let twice = format!("{text} {text}");
    s.apply(re, Edit::EditTextBlock { page: 0, block: 0, text: twice.clone(), style: style.clone() }).unwrap();
    assert_eq!(s.get(re).unwrap().text_blocks(0)[0].editor_text, twice);
    let again = s.save_bytes(re).unwrap();
    let next = s.open("twice.pdf", None, again, None).unwrap();
    assert_eq!(s.get(next).unwrap().text_blocks(0)[0].editor_text, twice);
    assert_eq!(pixels(s.get(next).unwrap().display.clone()), pixels(s.get(re).unwrap().display.clone()));
    s.apply(re, Edit::EditTextBlock { page: 0, block: 0, text: String::new(), style }).unwrap();
    assert!(s.get(re).unwrap().text_lines(0).is_empty(), "every contextual glyph is deleted");
    let empty = pixels(s.get(re).unwrap().display.clone());
    assert!(empty.as_chunks::<4>().0.iter().all(|p| p[..3] == [255; 3]));
    s.undo(id).unwrap();
    assert_eq!(pixels(s.get(id).unwrap().display.clone()), original);
}
#[test]
fn opentype_cff_document_font_preserves_program_codes_and_rendering_after_reopen() {
    exercise(support::font_fixture::cff_font(), "A A", "RectFont-Regular");
}
#[test]
fn contextual_arabic_marks_ligatures_and_mixed_bidi_keep_logical_text_after_reopen() {
    if let Some(path) = std::env::var_os("PDFCRAFT_TEST_ARABIC_FONT") {
        exercise(std::fs::read(path).unwrap(), "سَلَام 123", "NotoSansArabic-Regular");
        return;
    }
    let Some(font) = pdfcraft_fonts::document_arabic_font() else {
        eprintln!("set CRAFT_FONTS_DIR to test full Arabic glyph programs");
        return;
    };
    if pdfcraft_fonts::embedding::validate(font.data()).is_err() {
        eprintln!("set PDFCRAFT_TEST_ARABIC_FONT to an openly licensed static Noto Sans Arabic face");
        return;
    }
    exercise(font.data().to_vec(), "سَلَام 123", "NotoSansArabic-Regular");
}

#[test]
fn every_packaged_font_supports_its_glyphs_edit_add_save_reopen_and_edit_again() {
    if pdfcraft_fonts::CRAFT_FONTS.is_empty() {
        eprintln!("set CRAFT_FONTS_DIR for packaged-font save/reopen acceptance");
        return;
    }
    for (index, font) in pdfcraft_fonts::CRAFT_FONTS.iter().enumerate() {
        if let Some(source) = &font.served {
            assert!(font.bytes.is_empty(), "metadata must not embed the collection");
            font.register(std::fs::read(source.local_file).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", font.name()));
        }
        let covers = |ch: char| pdfcraft_fonts::embedding::covers(font.data(), &ch.to_string());
        let mut sample: String = "中文编辑ABC123".chars().filter(|ch| covers(*ch)).collect();
        if sample.is_empty() {
            use skrifa::MetadataProvider;
            sample = skrifa::FontRef::new(font.data())
                .unwrap()
                .charmap()
                .mappings()
                .filter(|(_, gid)| gid.to_u32() > 0)
                .filter_map(|(cp, _)| char::from_u32(cp))
                .filter(|ch| !ch.is_control() && !ch.is_whitespace())
                .take(6)
                .collect();
        }
        assert!(!sample.is_empty(), "{} has no testable glyph", font.name());
        let edit_text = sample.clone();
        let add_text: String = sample.chars().rev().collect();
        let mut replacement: String = "修改DEF456".chars().filter(|ch| covers(*ch)).collect();
        if replacement.is_empty() {
            replacement = sample.chars().rev().collect();
        }
        let mut session = Session::new();
        let id = session.open("source.pdf", None, document(), None).unwrap();
        let mut external = font.data().to_vec();
        external.push(0);
        if pdfcraft_fonts::embedding::validate_document_input(font.data()).is_err() {
            assert!(session.apply(id, Edit::RegisterTextFont { page: 0, bytes: Arc::new(external) }).is_err());
            assert!(!session.get(id).unwrap().dirty, "rejected external input changed the document");
        }
        assert!(
            session
                .apply(
                    id,
                    Edit::AddText {
                        page: 0,
                        text: pdfcraft_engine::AddedText {
                            rect: [30.0, 50.0, 270.0, 90.0],
                            text: sample.clone(),
                            family: pdfcraft_engine::FontFamily::Packaged(index),
                            bold: true,
                            ..Default::default()
                        },
                    }
                )
                .is_err(),
            "unsupported synthetic styles must not silently disappear"
        );
        assert!(!session.get(id).unwrap().dirty);
        let style = BlockStyle { resource: Some(pdfcraft_fonts::embedding::packaged_resource(font)), width: Some(240.0), ..Default::default() };
        session
            .apply(id, Edit::EditTextBlock { page: 0, block: 0, text: edit_text.clone(), style: style.clone() })
            .unwrap_or_else(|e| panic!("edit with {}: {e}", font.name()));
        session
            .apply(
                id,
                Edit::AddText {
                    page: 0,
                    text: pdfcraft_engine::AddedText {
                        rect: [30.0, 50.0, 270.0, 90.0],
                        text: add_text.clone(),
                        family: pdfcraft_engine::FontFamily::Packaged(index),
                        char_spacing: 1.5,
                        align: pdfcraft_engine::TextAlign::Right,
                        aligns: vec![Some(pdfcraft_engine::TextAlign::Center)],
                        ..Default::default()
                    },
                },
            )
            .unwrap_or_else(|e| panic!("add with {}: {e}", font.name()));
        let before = pixels(session.get(id).unwrap().display.clone());
        let saved = session.save_bytes(id).unwrap();
        let parsed = Document::open(saved.clone()).unwrap();
        let programs: Vec<_> = parsed
            .scan_objects()
            .filter_map(|(_, object)| match object.as_ref() {
                Object::Stream(stream) if stream.dict.get(b"PCFullFont").is_some() => Some(stream.decoded_within(font.data().len()).unwrap()),
                _ => None,
            })
            .collect();
        assert_eq!(programs.len(), 1, "{} must reuse its full program", font.name());
        assert_eq!(programs[0], font.data(), "{} original bytes changed", font.name());
        let reopened = session.open("saved.pdf", None, saved, None).unwrap();
        assert_eq!(pixels(session.get(reopened).unwrap().display.clone()), before, "{} rendering changed after reopen", font.name());
        let added = session.get(reopened).unwrap().added();
        let pdfcraft_engine::AddedContent::Text(added) = &added[0].content else { panic!("missing added text") };
        assert_eq!(added.family, pdfcraft_engine::FontFamily::Packaged(index));
        assert_eq!(added.text, add_text);
        assert_eq!(added.char_spacing, 1.5);
        assert_eq!(added.align, pdfcraft_engine::TextAlign::Right);
        assert_eq!(added.aligns, vec![Some(pdfcraft_engine::TextAlign::Center)]);
        let block = session.get(reopened).unwrap().text_blocks(0).iter().position(|b| b.editor_text == edit_text).unwrap();
        session
            .apply(reopened, Edit::EditTextBlock { page: 0, block, text: replacement.clone(), style: BlockStyle::default() })
            .unwrap_or_else(|e| panic!("re-edit with {}: {e}", font.name()));
        let again = session.save_bytes(reopened).unwrap();
        let next = session.open("again.pdf", None, again, None).unwrap();
        assert!(
            session.get(next).unwrap().text_blocks(0).iter().any(|b| b.editor_text == replacement),
            "{} re-edit expected {replacement:?}, got {:?}",
            font.name(),
            session.get(next).unwrap().text_blocks(0).iter().map(|b| b.editor_text.clone()).collect::<Vec<_>>()
        );
        eprintln!("edit/add/save/reopen/re-edit passed: {}", font.name());
    }
}
