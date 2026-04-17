use crate::renderer::Renderer;
use crate::PlainRenderer;
use effigy_core::widgets::{MessageBlock, SummaryCounts};

#[test]
fn renders_blocks_without_color_when_disabled() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);

    renderer
        .error_block(
            &MessageBlock::new("Task failed", "Unable to resolve task")
                .with_hint("Use `effigy tasks --task <name>`"),
        )
        .expect("render error block");

    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert_eq!(
        rendered,
        "[error] Task failed\n  Unable to resolve task\n  hint: Use `effigy tasks --task <name>`\n"
    );
}

#[test]
fn renders_section_and_summary_without_color_when_disabled() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);

    renderer.section("Task Catalogs").expect("section");
    renderer
        .summary(SummaryCounts {
            ok: 4,
            warn: 1,
            err: 0,
        })
        .expect("summary");

    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert_eq!(
        rendered,
        "Task Catalogs\n─────────────\nsummary  ok:4  warn:1  err:0\n"
    );
}
