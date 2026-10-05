# -*- coding: utf-8 -*-
"""生成 1024x1024 的应用图标源图。

形状参数严格照设计文档 9.6 节（该节已定稿）：
  圆角半径 0.35 * S，圆角曲线为超椭圆（指数 n = 4，不是正圆角）
  底色 #3451C6 纯色（不用渐变），前景纯白，右下柔投影
只依赖 Pillow，不联网、不读外部素材。

用法：python tools/make_icon.py
"""
import math
from PIL import Image, ImageDraw, ImageFilter

S = 1024  # 输出边长
SS = 4  # 超采样倍数，用于抗锯齿
P = S * SS  # 绘制画布边长

BG = (0x34, 0x51, 0xC6)
FG = (255, 255, 255)
SHADOW = (10, 16, 48)


def legacy_rounded_points(size, r_ratio=0.35, n=4.0, steps=48):
    """经典圆角轮廓：半径 r_ratio*size 的超椭圆圆角，指数 n。"""
    r = r_ratio * size
    exp = 2.0 / n
    pts = [(r, 0.0), (size - r, 0.0)]

    def corner(cx, cy, a0, a1):
        for i in range(1, steps + 1):
            a = a0 + (a1 - a0) * i / steps
            c, s = math.cos(a), math.sin(a)
            pts.append(
                (
                    cx + r * math.copysign(abs(c) ** exp, c),
                    cy + r * math.copysign(abs(s) ** exp, s),
                )
            )

    corner(size - r, r, -math.pi / 2, 0.0)
    pts.append((size, size - r))
    corner(size - r, size - r, 0.0, math.pi / 2)
    pts.append((r, size))
    corner(r, size - r, math.pi / 2, math.pi)
    pts.append((0.0, r))
    corner(r, r, math.pi, math.pi * 1.5)
    return pts


def catmull_rom_closed(pts, seg=14):
    """闭合 Catmull-Rom 样条插值：保证整条轮廓无折点。"""
    n = len(pts)
    out = []
    for i in range(n):
        p0, p1 = pts[(i - 1) % n], pts[i]
        p2, p3 = pts[(i + 1) % n], pts[(i + 2) % n]
        for s in range(seg):
            t = s / seg
            t2, t3 = t * t, t * t * t
            out.append(
                (
                    0.5
                    * (
                        (2 * p1[0])
                        + (-p0[0] + p2[0]) * t
                        + (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * t2
                        + (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * t3
                    ),
                    0.5
                    * (
                        (2 * p1[1])
                        + (-p0[1] + p2[1]) * t
                        + (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * t2
                        + (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * t3
                    ),
                )
            )
    return out


# 漏斗轮廓关键点（100x100 设计网格，顺时针）
FUNNEL_KEY = [
    (50, 15.5),
    (79, 23),
    (66, 44),
    (57.5, 59),
    (50, 63.5),
    (42.5, 59),
    (34, 44),
    (21, 23),
]
# 两个递减的小方块：x, y, 宽, 高, 圆角
BLOCKS = [(44.5, 68, 11, 11, 3.0), (47.0, 82, 6, 6, 1.8)]


def main():
    # 1) 底板：经典圆角 + 纯色
    plate = Image.new("RGBA", (P, P), (0, 0, 0, 0))
    ImageDraw.Draw(plate).polygon(legacy_rounded_points(P), fill=BG + (255,))

    # 2) 前景单独一层，方便做投影
    glyph = Image.new("RGBA", (P, P), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glyph)
    # 设计网格 100x100；图形由 y=15.5 到 y=88，共 72.5 格，占整幅 0.72
    scale = P * 0.72 / 72.5
    ox = P * 0.5 - 50 * scale
    oy = P * 0.5 - ((15.5 + 88) / 2) * scale

    def tf(pts):
        return [(ox + x * scale, oy + y * scale) for x, y in pts]

    gd.polygon(tf(catmull_rom_closed(FUNNEL_KEY, seg=14)), fill=FG + (255,))
    for x, y, w, h, r in BLOCKS:
        gd.rounded_rectangle(
            [
                ox + x * scale,
                oy + y * scale,
                ox + (x + w) * scale,
                oy + (y + h) * scale,
            ],
            radius=r * scale,
            fill=FG + (255,),
        )

    # 3) 投影：染色 -> 偏移 -> 模糊 -> 降透明度
    sh = Image.new("RGBA", (P, P), (0, 0, 0, 0))
    sh.paste(
        Image.new("RGBA", (P, P), SHADOW + (255,)),
        (int(0.0024 * P), int(0.024 * P)),
        glyph,
    )
    sh = sh.filter(ImageFilter.GaussianBlur(0.022 * P))
    sh.putalpha(sh.getchannel("A").point(lambda a: int(a * 0.42)))

    # 4) 合成并缩到目标尺寸
    canvas = Image.alpha_composite(plate, sh)
    canvas = Image.alpha_composite(canvas, glyph)
    out = canvas.resize((S, S), Image.Resampling.LANCZOS)
    out.save("src-tauri/icons/source.png")
    print("已生成 src-tauri/icons/source.png", out.size)

    # 设计文档 9.6 节要求的交付尺寸。tauri icon 已产出 32/64/128/256 等，
    # 这里补齐它不生成的那几档，保证尺寸清单完整。
    for px in (16, 48, 256, 512, 1024):
        out.resize((px, px), Image.Resampling.LANCZOS).save(
            f"src-tauri/icons/{px}x{px}.png"
        )
    print("已补齐图标尺寸：16 / 48 / 256 / 512 / 1024")


if __name__ == "__main__":
    main()
