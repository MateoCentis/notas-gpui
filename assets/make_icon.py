"""Genera el logo de Notas: assets/notas.ico (ícono del .exe) y assets/logo.png (barra de título).

Uso:  python assets/make_icon.py   (requiere Pillow)

El dibujo se hace a 4x y se reduce, para bordes suaves. En tamaños chicos se dibujan
menos filas para que siga leyéndose.
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

HERE = Path(__file__).parent
TOP_LEFT = (167, 139, 250)  # violeta claro  #A78BFA
BOTTOM_RIGHT = (99, 70, 229)  # índigo        #6346E5
CHECK = (99, 70, 229)


def gradient(n: int) -> Image.Image:
    """Degradado diagonal de arriba-izquierda a abajo-derecha."""
    small = Image.new("RGB", (2, 2))
    small.putpixel((0, 0), TOP_LEFT)
    small.putpixel((1, 1), BOTTOM_RIGHT)
    mid = tuple((a + b) // 2 for a, b in zip(TOP_LEFT, BOTTOM_RIGHT))
    small.putpixel((1, 0), mid)
    small.putpixel((0, 1), mid)
    return small.resize((n, n), Image.BILINEAR)


def draw(size: int) -> Image.Image:
    s = size * 4
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Fondo: cuadrado redondeado con degradado.
    mask = Image.new("L", (s, s), 0)
    pad = round(s * 0.04)
    ImageDraw.Draw(mask).rounded_rectangle(
        (pad, pad, s - pad, s - pad), radius=round(s * 0.24), fill=255
    )
    img.paste(gradient(s), (0, 0), mask)

    d = ImageDraw.Draw(img)
    rows = [0.34, 0.66] if size <= 24 else [0.30, 0.50, 0.70]
    box = s * (0.17 if size <= 24 else 0.13)
    stroke = max(4, round(s * (0.055 if size <= 24 else 0.04)))
    line_h = stroke * 1.25
    x0 = s * 0.22
    for i, cy in enumerate(rows):
        y = s * cy
        bx0, by0, bx1, by1 = x0, y - box / 2, x0 + box, y + box / 2
        r = box * 0.3
        done = i == 0
        if done:
            # Casilla marcada: blanca y con el check del color del fondo.
            d.rounded_rectangle((bx0, by0, bx1, by1), radius=r, fill="white")
            d.line(
                [
                    (bx0 + box * 0.22, y + box * 0.02),
                    (bx0 + box * 0.43, y + box * 0.22),
                    (bx0 + box * 0.80, y - box * 0.22),
                ],
                fill=CHECK,
                width=max(3, round(stroke * 0.85)),
                joint="curve",
            )
        else:
            d.rounded_rectangle(
                (bx0, by0, bx1, by1), radius=r, outline="white", width=round(stroke * 0.8)
            )
        # Renglón de texto; el de la tarea hecha, más tenue.
        lx0 = bx1 + s * 0.08
        lx1 = s * (0.78 if i != 1 else 0.70)
        alpha = 150 if done else 255
        layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
        ImageDraw.Draw(layer).rounded_rectangle(
            (lx0, y - line_h / 2, lx1, y + line_h / 2),
            radius=line_h / 2,
            fill=(255, 255, 255, alpha),
        )
        img = Image.alpha_composite(img, layer)
        d = ImageDraw.Draw(img)

    # Recortar todo a la forma del fondo.
    img.putalpha(ImageChops.multiply(img.getchannel("A"), mask))
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    images = [draw(n) for n in sizes]
    images[-1].save(
        HERE / "notas.ico",
        format="ICO",
        sizes=[(n, n) for n in sizes],
        append_images=images[:-1],
    )
    draw(64).save(HERE / "logo.png")
    draw(512).save(HERE / "logo-512.png")
    print("ok:", HERE / "notas.ico", HERE / "logo.png")


if __name__ == "__main__":
    main()
