"""Rebuild HomePanel's original Windows icon. Developer-only: requires Pillow."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1] / 'src-tauri' / 'icons'
root.mkdir(parents=True, exist_ok=True)
image = Image.new('RGBA', (512, 512), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((16, 16, 496, 496), 120, fill=(35, 166, 127, 255))
draw.line([(116, 245), (256, 119), (396, 245)], fill='white', width=25, joint='curve')
draw.line([(150, 224), (150, 387), (362, 387), (362, 224)], fill='white', width=25, joint='curve')
draw.line([(228, 387), (228, 292), (284, 292), (284, 387)], fill='white', width=22, joint='curve')
for size in (32, 128):
    image.resize((size, size), Image.Resampling.LANCZOS).save(root / f'{size}x{size}.png')
image.save(root / 'icon.ico', sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
