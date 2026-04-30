#!/usr/bin/env python3
"""Generate app icons. Design: rounded indigo gradient square with ascending bars + dot."""
from PIL import Image, ImageDraw, ImageFilter

SCALE = 8  # supersample for crisp downscaling

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))

def render(size: int) -> Image.Image:
    S = size * SCALE
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    radius = int(S * 0.22)

    # Diagonal gradient indigo -> violet
    top_color = (99, 102, 241, 255)      # indigo-500
    bottom_color = (139, 92, 246, 255)   # violet-500

    grad = Image.new("RGBA", (S, S), top_color)
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        t = y / (S - 1)
        gd.line([(0, y), (S, y)], fill=lerp(top_color, bottom_color, t))

    # Rounded mask
    mask = Image.new("L", (S, S), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle((0, 0, S - 1, S - 1), radius=radius, fill=255)

    base = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    base.paste(grad, (0, 0), mask)

    # Subtle inner top highlight
    hl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    hd = ImageDraw.Draw(hl)
    hd.rounded_rectangle((0, 0, S - 1, S - 1), radius=radius,
                         fill=(255, 255, 255, 28))
    hd.rounded_rectangle((0, int(S * 0.5), S - 1, S - 1), radius=radius,
                         fill=(0, 0, 0, 0))
    base = Image.alpha_composite(base, hl)

    # Ascending bars (3 vertical bars)
    bar_layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bd = ImageDraw.Draw(bar_layer)

    bar_w = int(S * 0.13)
    gap = int(S * 0.08)
    group_w = bar_w * 3 + gap * 2
    x_start = (S - group_w) // 2
    baseline = int(S * 0.74)
    heights = [0.22, 0.36, 0.52]  # ascending fractions of S
    bar_radius = int(bar_w * 0.35)

    for i, hf in enumerate(heights):
        x0 = x_start + i * (bar_w + gap)
        x1 = x0 + bar_w
        h = int(S * hf)
        y0 = baseline - h
        y1 = baseline
        bd.rounded_rectangle((x0, y0, x1, y1), radius=bar_radius,
                             fill=(255, 255, 255, 245))

    # Dot above tallest bar (data point)
    dot_r = int(S * 0.055)
    dot_cx = x_start + 2 * (bar_w + gap) + bar_w // 2
    dot_cy = baseline - int(S * heights[2]) - int(S * 0.075)
    bd.ellipse((dot_cx - dot_r, dot_cy - dot_r,
                dot_cx + dot_r, dot_cy + dot_r),
               fill=(255, 255, 255, 255))

    # Soft glow under bars
    glow = bar_layer.filter(ImageFilter.GaussianBlur(radius=int(S * 0.025)))
    base = Image.alpha_composite(base, glow)
    base = Image.alpha_composite(base, bar_layer)

    # Re-clip to rounded mask
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(base, (0, 0), mask)

    return out.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    import os, shutil, subprocess, tempfile

    icon_dir = "src-tauri/icons"
    os.makedirs(icon_dir, exist_ok=True)

    # Tauri's bundler reads these PNGs from tauri.conf.json
    png_targets = [
        (f"{icon_dir}/32x32.png", 32),
        (f"{icon_dir}/128x128.png", 128),
        (f"{icon_dir}/128x128@2x.png", 256),
        (f"{icon_dir}/icon.png", 512),  # generic high-res
    ]
    for path, size in png_targets:
        render(size).save(path, "PNG", optimize=True)
        print(f"wrote {path} ({size}x{size})")

    # Build a proper macOS .icns with all standard sizes
    iconset_sizes = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]

    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "icon.iconset")
        os.makedirs(iconset)
        for name, size in iconset_sizes:
            render(size).save(os.path.join(iconset, name), "PNG", optimize=True)
        out = f"{icon_dir}/icon.icns"
        subprocess.check_call(["iconutil", "-c", "icns", "-o", out, iconset])
        print(f"wrote {out}")

