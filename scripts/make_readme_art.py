#!/usr/bin/env python3
"""Build the README hero and feature collage from committed screenshots."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "docs" / "screenshots"
OUT = ROOT / "docs" / "assets"
FONT_DIR = ROOT / "assets" / "fonts"


def font(name: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONT_DIR / name), size)


def gradient(size: tuple[int, int], top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    w, h = size
    out = Image.new("RGB", size)
    draw = ImageDraw.Draw(out)
    for y in range(h):
        t = y / max(1, h - 1)
        draw.line(
            [(0, y), (w, y)],
            fill=tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)),
        )
    return out


def grid(canvas: Image.Image, step: int = 48, color: tuple[int, int, int, int] = (255, 255, 255, 12)) -> None:
    layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for x in range(0, canvas.width, step):
        d.line([(x, 0), (x, canvas.height)], fill=color, width=1)
    for y in range(0, canvas.height, step):
        d.line([(0, y), (canvas.width, y)], fill=color, width=1)
    canvas.alpha_composite(layer)


def card(
    canvas: Image.Image,
    shot: str,
    box: tuple[int, int, int, int],
    label: str,
    radius: int = 18,
) -> None:
    x0, y0, x1, y1 = box
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.rounded_rectangle((x0 + 8, y0 + 16, x1 + 8, y1 + 16), radius=radius, fill=(0, 0, 0, 150))
    shadow = shadow.filter(ImageFilter.GaussianBlur(18))
    canvas.alpha_composite(shadow)

    image = Image.open(SHOTS / shot).convert("RGB")
    target = (x1 - x0, y1 - y0 - 34)
    source_ratio = image.width / image.height
    target_ratio = target[0] / target[1]
    if source_ratio > target_ratio:
        new_w = round(image.height * target_ratio)
        image = image.crop(((image.width - new_w) // 2, 0, (image.width + new_w) // 2, image.height))
    else:
        new_h = round(image.width / target_ratio)
        image = image.crop((0, (image.height - new_h) // 2, image.width, (image.height + new_h) // 2))
    image = image.resize(target, Image.Resampling.LANCZOS)

    mask = Image.new("L", (target[0], target[1] + 34), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, target[0] - 1, target[1] + 33), radius=radius, fill=255)
    panel = Image.new("RGBA", (target[0], target[1] + 34), (22, 27, 42, 255))
    panel.paste(image, (0, 0))
    pd = ImageDraw.Draw(panel)
    pd.text((16, target[1] + 7), label, font=font("Inter-Medium.ttf", 15), fill=(226, 232, 240, 255))
    canvas.alpha_composite(panel, (x0, y0))


def hero() -> None:
    w, h = 1600, 720
    canvas = gradient((w, h), (10, 15, 30), (20, 28, 48)).convert("RGBA")
    grid(canvas)

    d = ImageDraw.Draw(canvas)
    d.rounded_rectangle((66, 58, 248, 92), radius=17, fill=(70, 235, 190, 34), outline=(70, 235, 190, 120), width=2)
    d.text((86, 66), "LOCAL-FIRST CANVAS", font=font("Inter-SemiBold.ttf", 15), fill=(112, 245, 204, 255))
    d.text((64, 122), "Sketchforge", font=font("Inter-SemiBold.ttf", 86), fill=(247, 249, 252, 255))
    d.text((68, 228), "Think in infinite space.", font=font("Inter-Regular.ttf", 34), fill=(169, 181, 201, 255))
    d.text(
        (68, 280),
        "A native Rust canvas for diagrams, whiteboards, and vector ideas.",
        font=font("Inter-Regular.ttf", 22),
        fill=(137, 150, 173, 255),
    )

    card(canvas, "app_demo.png", (690, 88, 1510, 398), "Canvas + design inspector")
    card(canvas, "milestone4_whiteboard.png", (742, 322, 1534, 622), "Whiteboard mode")
    card(canvas, "milestone7_mcp_live_xvfb.png", (650, 430, 1130, 674), "MCP-driven automation")

    canvas.convert("RGB").save(OUT / "hero.webp", "WEBP", quality=86, method=6)


def features() -> None:
    w, h = 1200, 900
    canvas = gradient((w, h), (13, 19, 34), (23, 29, 46)).convert("RGBA")
    grid(canvas, step=40)
    d = ImageDraw.Draw(canvas)
    d.text((48, 36), "Built for real work", font=font("Inter-SemiBold.ttf", 42), fill=(247, 249, 252, 255))
    d.text(
        (50, 92),
        "Sketching, structure, and handoff in one local file format.",
        font=font("Inter-Regular.ttf", 20),
        fill=(154, 166, 187, 255),
    )

    card(canvas, "milestone3_figma_core.png", (48, 150, 588, 420), "Frames, vectors, text, constraints")
    card(canvas, "milestone5_inspector.png", (612, 150, 1152, 420), "Detailed design inspector")
    card(canvas, "milestone2_pages.png", (48, 452, 460, 708), "Projects and pages")
    card(canvas, "milestone6_svg_import.png", (482, 452, 894, 708), "SVG and Excalidraw import")
    card(canvas, "milestone6_pdf_page.png", (916, 452, 1152, 708), "PDF export")

    canvas.convert("RGB").save(OUT / "features.webp", "WEBP", quality=86, method=6)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    hero()
    features()
    for name in ("hero.webp", "features.webp"):
        path = OUT / name
        print(f"{path.relative_to(ROOT)} {path.stat().st_size // 1024} KiB")
