use crate::ui::renderer::Renderer;
use crate::ui::{PlainRenderer, TableSpec};

#[test]
fn renders_bullet_list_and_table_without_color_when_disabled() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    renderer
        .bullet_list(
            "evidence",
            &[
                "Detected root markers: package.json".to_owned(),
                "effigy link present: no".to_owned(),
            ],
        )
        .expect("bullet list");
    renderer
        .table(&TableSpec::new(
            vec!["catalog".to_owned(), "task".to_owned()],
            vec![vec!["root".to_owned(), "dev".to_owned()]],
        ))
        .expect("table");

    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("evidence:\n- Detected root markers: package.json"));
    assert!(rendered.contains("catalog"));
    assert!(rendered.contains("root"));
    assert!(rendered.contains("dev"));
}
