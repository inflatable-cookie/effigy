use crate::renderer::Renderer;
use crate::PlainRenderer;

#[test]
fn spinner_falls_back_to_step_output_when_progress_disabled() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false).with_progress_enabled(false);

    let spinner = renderer.spinner("Scanning workspace").expect("spinner");
    spinner.set_message("Still scanning");
    spinner.finish_success("Done");

    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert_eq!(rendered, "◌ Scanning workspace\n");
}
