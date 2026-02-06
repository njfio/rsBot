use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pi_tui::{apply_overlay, Component, DiffRenderer, EditorBuffer, EditorView, LumaImage};

fn benchmark_renderer_diff(c: &mut Criterion) {
    let mut renderer = DiffRenderer::new();
    let initial = (0..200)
        .map(|index| format!("line-{index:03}: {}", "x".repeat(40)))
        .collect::<Vec<_>>();
    let mut next = initial.clone();
    next[123] = "line-123: changed".to_string();

    c.bench_function("diff_renderer_200_lines_single_change", |b| {
        b.iter(|| {
            let _ = renderer.diff(black_box(initial.clone()));
            let _ = renderer.diff(black_box(next.clone()));
        })
    });
}

fn benchmark_editor_view(c: &mut Criterion) {
    let text = (0..500)
        .map(|index| format!("editor line {}", index))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = EditorBuffer::from_text(&text);
    let view = EditorView::new(&buffer).with_viewport(200, 40);

    c.bench_function("editor_view_render_40_lines", |b| {
        b.iter(|| black_box(view.render(120)))
    });
}

fn benchmark_overlay_and_image(c: &mut Criterion) {
    let base = (0..80)
        .map(|index| format!("base row {index:02}: {}", " ".repeat(120)))
        .collect::<Vec<_>>();
    let image = LumaImage::from_luma(128, 64, vec![180; 128 * 64]).expect("image");
    let image_lines = image.render(80);

    c.bench_function("overlay_ascii_image_on_canvas", |b| {
        b.iter(|| black_box(apply_overlay(&base, &image_lines, 8, 20)))
    });
}

criterion_group!(
    benches,
    benchmark_renderer_diff,
    benchmark_editor_view,
    benchmark_overlay_and_image
);
criterion_main!(benches);
