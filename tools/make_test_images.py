"""生成一批受控测试图片，用于图筛的端到端验证。

刻意构造出：一模一样组、同图缩略版（相似）、灰阶图、彩色图、同作品不同页、损坏文件、非图片文件。
只依赖 Pillow，全部程序生成，不读用户真实图片。
"""
import os
import sys
from PIL import Image, ImageDraw

ROOT = sys.argv[1] if len(sys.argv) > 1 else r"D:\code\PicSieve\.e2e\images"
SUB = os.path.join(ROOT, "sub")

os.makedirs(SUB, exist_ok=True)
for f in os.listdir(ROOT):
    p = os.path.join(ROOT, f)
    if os.path.isfile(p):
        os.remove(p)


def busy(w, h, seed=0):
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(0, h, 8):
        for x in range(0, w, 8):
            r = (x * 255 // max(w, 1) + seed) % 256
            g = y * 255 // max(h, 1)
            b = 40 if ((x // 8 + y // 8) % 2 == 0) else 200
            d.rectangle([x, y, x + 7, y + 7], fill=(r, g, b))
    return img


# 1) 一模一样组：同一份内容存两份（文件名形状照实测样本）
a = busy(600, 800)
a.save(os.path.join(ROOT, "#1 - 白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] -.png"))
a.save(os.path.join(SUB, "#1 - 白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] -.png"))

# 2) 相似：同图缩到 55%（视觉相同、尺寸不同）
a.resize((330, 440), Image.LANCZOS).save(
    os.path.join(ROOT, "#1 - 白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] - small.png")
)

# 3) 灰阶图（黑白灰）
gray = Image.new("RGB", (500, 700), (128, 128, 128))
d = ImageDraw.Draw(gray)
d.rectangle([50, 50, 250, 350], fill=(0, 0, 0))
d.rectangle([250, 350, 450, 650], fill=(255, 255, 255))
gray.save(os.path.join(ROOT, "#1 - モノクロ - 灰テスト - PID=223344556 - gray.png"))

# 4) 彩色图（明显不是灰阶）
busy(900, 1200, seed=90).save(
    os.path.join(ROOT, "#1 - 色の絵 - カラー氏 - [pid=334455667] - color.jpg"), quality=88
)

# 5) 同作品不同页：pid 相同、_p0 / _p1
p0 = busy(400, 400, seed=10)
p1 = busy(400, 400, seed=200)
p0.save(os.path.join(ROOT, "11223344_p0.png"))
p1.save(os.path.join(ROOT, "11223344_p1.png"))

# 6) 损坏图片（扩展名是图片，内容不是）
with open(os.path.join(ROOT, "broken.jpg"), "wb") as fh:
    fh.write(b"\xFF\xD8\xFF\xE0garbage-not-an-image")

# 7) 非图片文件：扫描应完全忽略
with open(os.path.join(ROOT, "note.txt"), "w", encoding="utf-8") as fh:
    fh.write("我不是图片")

names = sorted(os.listdir(ROOT))
print("生成于", ROOT)
for n in names:
    print("  ", n)
print("子目录:", sorted(os.listdir(SUB)))
