#!/usr/bin/env python3
"""Generate macOS menu bar template tray icon — monochrome with alpha.
The system inverts to white in dark menu bars and dims when not active.
Design: ascending bars matching the app icon."""
from PIL import Image, ImageDraw

SCALE = 8

def render(size: int) -> Image.Image:
    S = size * SCALE
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Three ascending bars, slightly more compact than app icon since
    # tray icons render small. Pure black with alpha = template image.
    bar_w = int(S * 0.16)
    gap = int(S * 0.10)
    group_w = bar_w * 3 + gap * 2
    x_start = (S - group_w) // 2
    baseline = int(S * 0.78)
    heights = [0.28, 0.46, 0.64]
    bar_radius = int(bar_w * 0.32)

    for i, hf in enumerate(heights):
        x0 = x_start + i * (bar_w + gap)
        x1 = x0 + bar_w
        y0 = baseline - int(S * hf)
        d.rounded_rectangle((x0, y0, x1, baseline),
                            radius=bar_radius, fill=(0, 0, 0, 255))

    # Dot above the tallest bar
    dot_r = int(S * 0.07)
    dot_cx = x_start + 2 * (bar_w + gap) + bar_w // 2
    dot_cy = baseline - int(S * heights[2]) - int(S * 0.10)
    d.ellipse((dot_cx - dot_r, dot_cy - dot_r,
               dot_cx + dot_r, dot_cy + dot_r),
              fill=(0, 0, 0, 255))

    return img.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    # macOS menu bar wants 22pt icons → 22 (1x) and 44 (2x). Tauri picks one.
    # Filename ending in `Template` makes macOS auto-invert.
    targets = [
        ("src-tauri/icons/trayIconTemplate.png", 22),
        ("src-tauri/icons/trayIconTemplate@2x.png", 44),
    ]
    for path, size in targets:
        render(size).save(path, "PNG", optimize=True)
        print(f"wrote {path} ({size}x{size})")
