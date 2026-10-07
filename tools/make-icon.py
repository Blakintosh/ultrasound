"""Builds Ultrasound's Windows icon from the master logo.

    python tools/make-icon.py [master]      (default: branding/ultrasound-logo-master.png)

Writes assets/ultrasound.ico with every size Windows asks for (Explorer, the taskbar and the console's title bar at
100-300 % scaling), each resampled from the master rather than from one another. build.rs embeds it in the exe. Run
this again whenever the master changes.
"""
import sys
from pathlib import Path
from PIL import Image, ImageFilter

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
MARGIN = 0.04  # of the square's side, each edge: the mark fills the tile as Windows' own icons do
# Up to this size the black hull sinks into a dark taskbar (#202020): a 1 px keyline in the logo's own grey, outside
# its black outline, keeps the silhouette. Larger frames are the master as drawn.
KEYLINE_MAX = 48
KEYLINE = (0x6E, 0x6E, 0x6E, 170)

root = Path(__file__).resolve().parent.parent
src = Image.open(sys.argv[1] if len(sys.argv) > 1 else root / "branding" / "ultrasound-logo-master.png").convert("RGBA")
# Solid body, antialiased edge.
src.putalpha(src.getchannel("A").point(lambda a: 255 if a >= 248 else a))
src = src.crop(src.getbbox())

side = round(max(src.size) / (1 - 2 * MARGIN))
master = Image.new("RGBA", (side, side), (0, 0, 0, 0))
master.paste(src, ((side - src.width) // 2, (side - src.height) // 2), src)


def render(size: int) -> Image.Image:
    # Premultiplied, so the transparent surround doesn't bleed a dark fringe into the edge.
    img = master.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")
    if size <= 32:
        # At title-bar sizes the waves and hull lines blur; a light unsharp keeps them apart.
        rgb = img.convert("RGB").filter(ImageFilter.UnsharpMask(radius=0.6, percent=70, threshold=0))
        rgb.putalpha(img.getchannel("A"))
        img = rgb
    if size <= KEYLINE_MAX:
        ring = img.getchannel("A").filter(ImageFilter.MaxFilter(3))
        keyline = Image.new("RGBA", img.size, KEYLINE)
        keyline.putalpha(ring.point(lambda v: v * KEYLINE[3] // 255))
        img = Image.alpha_composite(keyline, img)
    return img


out = root / "assets" / "ultrasound.ico"
out.parent.mkdir(exist_ok=True)
frames = [render(s) for s in ICO_SIZES]
frames[-1].save(out, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])
print("ultrasound.ico:", sorted(Image.open(out).info["sizes"]), f"{out.stat().st_size // 1024} KB")
