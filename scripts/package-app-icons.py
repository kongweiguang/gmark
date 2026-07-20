# @author kongweiguang

"""Packages generated gmark PNGs for Windows, macOS, and Linux."""

from pathlib import Path
from PIL import Image


ROOT = Path(__file__).resolve().parent.parent
ICON_DIR = ROOT / "assets" / "icon"
SIZES = (16, 32, 48, 64, 128, 256, 512)


def main() -> None:
    images = [Image.open(ICON_DIR / f"gmark-icon-{size}.png").convert("RGBA") for size in SIZES]
    images[-1].save(
        ICON_DIR / "gmark.ico",
        format="ICO",
        append_images=images[:-1],
        sizes=[(size, size) for size in SIZES],
    )
    Image.open(ICON_DIR / "gmark-icon.png").convert("RGBA").save(
        ROOT / "resources" / "macos" / "gmark.icns",
        format="ICNS",
    )
    for size in (256, 512):
        target = (
            ROOT
            / "resources"
            / "linux"
            / "icons"
            / "hicolor"
            / f"{size}x{size}"
            / "apps"
            / "com.kongweiguang.gmark.png"
        )
        target.write_bytes((ICON_DIR / f"gmark-icon-{size}.png").read_bytes())


if __name__ == "__main__":
    main()
