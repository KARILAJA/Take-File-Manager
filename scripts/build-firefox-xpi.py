import json
import re
import shutil
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC_EXT = ROOT / "extension"
OUT_DIR = ROOT / "extension-firefox"
OUT_XPI = ROOT / "take-file-manager-firefox.xpi"

FIREFOX_GECKO_ID = "takefilemanager@turboget.app"


def build():
    if OUT_DIR.exists():
        shutil.rmtree(OUT_DIR)
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    for rel_item in ["background.js", "content_magnet.js", "content_video.js", "icons", "popup", "options"]:
        src_path = SRC_EXT / rel_item
        dst_path = OUT_DIR / rel_item
        if src_path.is_dir():
            shutil.copytree(src_path, dst_path)
        elif src_path.is_file():
            shutil.copy2(src_path, dst_path)

    # Strip Chromium-only downloads.onDeterminingFilename block so AMO addons-linter reports 0 warnings
    bg_path = OUT_DIR / "background.js"
    bg_text = bg_path.read_text(encoding="utf-8")
    bg_text = re.sub(
        r"\r?\n\s*if \(chrome\.downloads\?\.onDeterminingFilename\) \{[\s\S]*?\}\r?\n",
        "\n",
        bg_text,
    )
    bg_path.write_text(bg_text, encoding="utf-8")

    manifest = json.loads((SRC_EXT / "manifest.json").read_text(encoding="utf-8"))
    manifest.pop("key", None)
    manifest["background"] = {"scripts": ["background.js"]}
    manifest["browser_specific_settings"] = {
        "gecko": {
            "id": FIREFOX_GECKO_ID,
            "strict_min_version": "140.0",
            "data_collection_permissions": {
                "required": ["none"]
            },
        },
        "gecko_android": {
            "strict_min_version": "142.0"
        },
    }

    (OUT_DIR / "manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    if OUT_XPI.exists():
        OUT_XPI.unlink()

    with zipfile.ZipFile(OUT_XPI, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
        for file_path in sorted(OUT_DIR.rglob("*")):
            if file_path.is_file():
                arcname = file_path.relative_to(OUT_DIR).as_posix()
                zf.write(file_path, arcname)

    print(f"Created Firefox directory: {OUT_DIR}")
    print(f"Created Firefox XPI package: {OUT_XPI} ({OUT_XPI.stat().st_size} bytes)")


if __name__ == "__main__":
    build()
