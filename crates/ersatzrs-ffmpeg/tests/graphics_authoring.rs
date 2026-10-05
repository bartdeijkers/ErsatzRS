use std::collections::BTreeMap;

use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ScriptGraphicsConfig,
};
use ersatzrs_ffmpeg::{FrameSize, GraphicsRenderContext, GraphicsTemplateRenderer};

#[test]
fn authored_script_execution_is_bounded_without_replacing_the_native_renderer() {
    let directory = tempfile::tempdir().unwrap();
    let renderer = GraphicsTemplateRenderer::new(directory.path());
    let context = GraphicsRenderContext::new(FrameSize::new(320, 180), FrameSize::new(320, 180));
    let mut element = GraphicsElement {
        name: "Bounded native script".into(),
        kind: GraphicsElementKind::Script,
        config: GraphicsElementConfig::Script(ScriptGraphicsConfig {
            command: Some("while true do end".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let error = renderer
        .render_graphics_elements(std::slice::from_ref(&element), &BTreeMap::new(), &context)
        .unwrap_err();
    assert!(error.to_string().contains("instruction"), "{error}");
    for command in [
        "local co = coroutine.create(function() for i=1,2000000 do local x=i*i end end); local ok,err=coroutine.resume(co); if not ok then error(err) end; return 'rect 8 8 80 80 fill'",
        "return string.rep('x', 32*1024*1024)",
    ] {
        let GraphicsElementConfig::Script(config) = &mut element.config else { unreachable!() };
        config.command = Some(command.into());
        assert!(renderer.render_graphics_elements(std::slice::from_ref(&element), &BTreeMap::new(), &context).is_err());
    }
    let large_source = directory.path().join("large.lua");
    std::fs::write(&large_source, " ".repeat(64 * 1024 + 1)).unwrap();
    let GraphicsElementConfig::Script(config) = &mut element.config else {
        unreachable!()
    };
    config.command = Some(large_source.to_string_lossy().into_owned());
    assert!(renderer
        .render_graphics_elements(std::slice::from_ref(&element), &BTreeMap::new(), &context)
        .unwrap_err()
        .to_string()
        .contains("64 KiB"));
    let GraphicsElementConfig::Script(config) = &mut element.config else {
        unreachable!()
    };
    config.command = Some("assert(type(os.date) == 'function' and type(os.time) == 'function' and os.execute == nil and os.exit == nil and os.remove == nil and io == nil and package == nil and dofile == nil and loadfile == nil); return 'rect 8 8 80 80 setcolor green fill'".into());
    assert_eq!(
        renderer
            .render_graphics_elements(&[element], &BTreeMap::new(), &context)
            .unwrap()
            .len(),
        1
    );
}
