# Normalize the generated app icon to macOS geometry and export an .icns.
# Usage: uv run --with pillow tmp/make_icon.py <raw.png> <out-dir>
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

CANVAS = 1024
TILE = round(CANVAS * 0.805)  # 824: tile size of real macOS 26 icons
MARGIN = (CANVAS - TILE) // 2  # 100


def squircle_mask(size: int, n: float = 5.0, oversample: int = 4) -> Image.Image:
    """Antialiased superellipse |x|^n + |y|^n <= 1, the continuous-corner macOS tile shape."""
    big = size * oversample
    mask = Image.new("L", (big, big), 0)
    pts = []
    import math

    steps = 2000
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        x = math.copysign(abs(c) ** (2 / n), c)
        y = math.copysign(abs(s) ** (2 / n), s)
        pts.append(((x + 1) / 2 * (big - 1), (y + 1) / 2 * (big - 1)))
    ImageDraw.Draw(mask).polygon(pts, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def clean(img: Image.Image) -> Image.Image:
    """Zero the sub-1% alpha tails that resampling leaves on the canvas border."""
    img = img.copy()
    img.putalpha(img.getchannel("A").point(lambda v: 0 if v < 4 else v))
    return img


def small_icon(size: int) -> Image.Image:
    """Pixel-grid icon for 16 and 32 px: the same tree, redrawn so every gap is a whole pixel."""
    u = size // 16  # 1 at 16 px, 2 at 32 px
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    tile_size, off = 14 * u, 1 * u
    # Blue tile with a vertical gradient, macOS corners.
    grad = Image.new("RGBA", (tile_size, tile_size))
    top, bottom = (0x3E, 0x7B, 0xFA), (0x23, 0x48, 0xD6)
    for y in range(tile_size):
        t = y / max(tile_size - 1, 1)
        grad.paste(tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,), (0, y, tile_size, y + 1))
    grad.putalpha(squircle_mask(tile_size, oversample=8))
    img.alpha_composite(grad, (off, off))
    d = ImageDraw.Draw(img)
    glass = (232, 240, 255, 255)
    cyan = (95, 245, 224, 255)

    def rect(x0: int, y0: int, x1: int, y1: int, color) -> None:  # grid units, inclusive
        d.rectangle((x0 * u, y0 * u, (x1 + 1) * u - 1, (y1 + 1) * u - 1), fill=color)

    rect(3, 4, 3, 11, glass)  # stem
    for y0, x1 in ((4, 12), (7, 11), (10, 9)):  # three equal bars, 2 units tall, 1-unit gaps
        rect(4, y0, 4, y0, glass)  # connector
        rect(5, y0, x1, y0 + 1, glass)
    # Pulse: a cyan crest rising out of the top bar.
    rect(6, 5, 7, 5, cyan)
    rect(8, 3, 8, 4, cyan)
    rect(9, 5, 11, 5, cyan)
    return img


def main() -> None:
    raw = Image.open(sys.argv[1]).convert("RGBA")
    out = Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)

    # The tile: everything that is (nearly) opaque. Ignore faint shadow/residue.
    alpha = raw.getchannel("A").point(lambda a: 255 if a > 200 else 0)
    box = alpha.getbbox()
    # Cut the generator's own outer rim (a dark lower lip and a thick bright bevel), ~1.5% per side.
    inset = round((box[2] - box[0]) * 0.015)
    box = (box[0] + inset, box[1] + inset, box[2] - inset, box[3] - inset)
    tile = raw.crop(box).resize((TILE, TILE), Image.LANCZOS)
    # The generated tile's corners are rounder than the macOS squircle; fill what lies between them
    # with the tile's own blue gradient instead of transparent black. The gradient is sampled down
    # the right-hand margin column, where the symbol is not.
    base = Image.new("RGBA", (TILE, TILE))
    for y in range(TILE):
        r, g, b, _ = tile.getpixel((TILE - 70, min(max(y, 40), TILE - 41)))
        base.paste((r, g, b, 255), (0, y, TILE, y + 1))
    base.alpha_composite(tile)
    tile = base

    # Opaque only where both the squircle and the generated tile are solid: the squircle enforces
    # the macOS corners, the tile's own alpha keeps its transparent corners from showing as black.
    # Near-opaque interior pixels (the generator leaves ~253) become fully opaque.
    mask = squircle_mask(TILE)
    rgb = tile.convert("RGB")
    tile = Image.merge("RGBA", (*rgb.split(), mask))

    # Soft neutral shadow, slightly offset downward, then the tile on top.
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    shadow = Image.new("L", (CANVAS, CANVAS), 0)
    shadow.paste(mask.point(lambda a: a * 0.32), (MARGIN, MARGIN + 10))
    shadow = shadow.filter(ImageFilter.GaussianBlur(18))
    canvas.paste((0, 0, 0, 255), (0, 0), shadow)
    canvas.alpha_composite(tile, (MARGIN, MARGIN))
    # Drop sub-1% residue left by the blur at the far edges.
    a = canvas.getchannel("A").point(lambda v: 0 if v < 3 else v)
    canvas.putalpha(a)
    canvas.save(out / "AppIcon.png")

    # .iconset -> .icns
    iconset = out / "AppIcon.iconset"
    iconset.mkdir(exist_ok=True)
    def export(px: int) -> Image.Image:
        return small_icon(px) if px in (16, 32) else clean(canvas.resize((px, px), Image.LANCZOS))

    for size in (16, 32, 128, 256, 512):
        export(size).save(iconset / f"icon_{size}x{size}.png")
        export(size * 2).save(iconset / f"icon_{size}x{size}@2x.png")
    for px in (16, 32):
        small_icon(px).resize((px * 8, px * 8), Image.NEAREST).save(out / f"preview_{px}px_x8.png")
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(out / "AppIcon.icns")], check=True)

    # Interior check: a central region well inside the rounded corners.
    tile_alpha = canvas.getchannel("A").crop((MARGIN + 200, MARGIN + 40, MARGIN + TILE - 200, MARGIN + TILE - 40))
    dark = sum(
        1
        for x in range(MARGIN, MARGIN + TILE, 2)
        for y in range(MARGIN, MARGIN + TILE, 2)
        if (p := canvas.getpixel((x, y)))[3] > 128 and max(p[:3]) < 40
    )
    print("near-black opaque pixels on the tile (sampled):", dark)
    print("tile box in raw image:", box, "->", f"{TILE}x{TILE} at margin {MARGIN}")
    print("tile interior alpha min:", tile_alpha.getextrema()[0])
    print("corner pixel alpha:", canvas.getpixel((0, 0))[3], "edge-midpoint alpha:", canvas.getpixel((CANVAS // 2, 2))[3])
    print("wrote", out / "AppIcon.png", "and", out / "AppIcon.icns")


main()
