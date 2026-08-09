#!/usr/bin/env python3
"""Build the Kendr Optimizer technical whitepaper from Markdown.

The builder intentionally depends only on ReportLab. It supports the subset of
Markdown used by docs/whitepaper.md and adds vector figures at explicit
``pdf-figure`` markers. The Markdown remains the authoritative prose source.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import re
from pathlib import Path
from typing import Iterable

from reportlab import Version as REPORTLAB_VERSION
from reportlab.graphics.shapes import Drawing, Line, Rect, String
from reportlab.lib import colors
from reportlab.lib.colors import HexColor
from reportlab.lib.enums import TA_CENTER, TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfgen import canvas as pdf_canvas
from reportlab.platypus import (
    BaseDocTemplate,
    CondPageBreak,
    Flowable,
    Frame,
    HRFlowable,
    ListFlowable,
    ListItem,
    NextPageTemplate,
    PageBreak,
    PageTemplate,
    Paragraph,
    Spacer,
    Table,
    TableStyle,
    XPreformatted,
)
from reportlab.platypus.tableofcontents import TableOfContents


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = ROOT / "docs" / "whitepaper.md"
DEFAULT_OUTPUT = (
    ROOT
    / "output"
    / "pdf"
    / "kendr-optimizer-verification-gated-token-reduction-whitepaper.pdf"
)
BRAND_MARK = ROOT / "docs" / "assets" / "kendr-mark-white-512.png"
BRAND_ICON = ROOT / "docs" / "assets" / "kendr-icon-512.png"
FONT_DIR = ROOT / "docs" / "assets" / "fonts"

PAGE_WIDTH, PAGE_HEIGHT = A4
MARGIN_X = 18 * mm
MARGIN_TOP = 19 * mm
MARGIN_BOTTOM = 18 * mm
CONTENT_WIDTH = PAGE_WIDTH - 2 * MARGIN_X

# Kendr's official warm palette, mirrored from kendr.org and the brand pack.
INK = HexColor("#2B2925")
MUTED = HexColor("#6B6459")
PAPER = HexColor("#FAF8F4")
SURFACE = HexColor("#F3EFE8")
NAVY = HexColor("#151412")
BLUE = HexColor("#B8551A")
CYAN = HexColor("#E2712A")
GREEN = HexColor("#4D6B42")
AMBER = HexColor("#9C4614")
RED = HexColor("#96322A")
LAVENDER = HexColor("#8A8378")
LIGHT_BLUE = HexColor("#F7E8DC")
LIGHT_GREEN = HexColor("#E8EEE4")
LIGHT_AMBER = HexColor("#F4E9DC")
LIGHT_RED = HexColor("#F2E2DF")
GRID = HexColor("#D8D1C8")
DARK_SURFACE = HexColor("#1A1917")
DARK_RAISED = HexColor("#3A3630")
DARK_TEXT = HexColor("#D6D3D1")
DARK_SECONDARY = HexColor("#C2BDB6")


def register_fonts() -> tuple[str, str, str, str, str, str]:
    """Register vendored brand fonts, with a readable development fallback."""

    vendored = {
        "regular": FONT_DIR / "Inter-Regular.ttf",
        "bold": FONT_DIR / "Inter-SemiBold.ttf",
        "italic": FONT_DIR / "Inter-Italic.ttf",
        "heading": FONT_DIR / "SpaceGrotesk-SemiBold.ttf",
        "mono": FONT_DIR / "CascadiaMono-Regular.ttf",
    }
    if all(path.is_file() for path in vendored.values()):
        pdfmetrics.registerFont(TTFont("KendrSans", str(vendored["regular"])))
        pdfmetrics.registerFont(TTFont("KendrSans-Bold", str(vendored["bold"])))
        pdfmetrics.registerFont(TTFont("KendrSans-Italic", str(vendored["italic"])))
        pdfmetrics.registerFont(TTFont("KendrDisplay", str(vendored["heading"])))
        pdfmetrics.registerFont(TTFont("KendrMono", str(vendored["mono"])))
        pdfmetrics.registerFontFamily(
            "KendrSans",
            normal="KendrSans",
            bold="KendrSans-Bold",
            italic="KendrSans-Italic",
            boldItalic="KendrSans-Bold",
        )
        return (
            "KendrSans",
            "KendrSans-Bold",
            "KendrSans-Italic",
            "KendrDisplay",
            "KendrMono",
            "vendored Inter/Space Grotesk/Cascadia Mono",
        )

    candidates = [
        (
            Path("C:/Windows/Fonts/arial.ttf"),
            Path("C:/Windows/Fonts/arialbd.ttf"),
            Path("C:/Windows/Fonts/ariali.ttf"),
            Path("C:/Windows/Fonts/consola.ttf"),
        ),
        (
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"),
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        ),
    ]
    for regular, bold, italic, mono in candidates:
        if all(path.is_file() for path in (regular, bold, italic, mono)):
            pdfmetrics.registerFont(TTFont("KendrSans", str(regular)))
            pdfmetrics.registerFont(TTFont("KendrSans-Bold", str(bold)))
            pdfmetrics.registerFont(TTFont("KendrSans-Italic", str(italic)))
            pdfmetrics.registerFont(TTFont("KendrMono", str(mono)))
            pdfmetrics.registerFontFamily(
                "KendrSans",
                normal="KendrSans",
                bold="KendrSans-Bold",
                italic="KendrSans-Italic",
                boldItalic="KendrSans-Bold",
            )
            return (
                "KendrSans",
                "KendrSans-Bold",
                "KendrSans-Italic",
                "KendrSans-Bold",
                "KendrMono",
                f"{regular.name}/{mono.name}",
            )
    return (
        "Helvetica",
        "Helvetica-Bold",
        "Helvetica-Oblique",
        "Helvetica-Bold",
        "Courier",
        "PDF base Helvetica/Courier",
    )


FONT, FONT_BOLD, FONT_ITALIC, FONT_HEADING, FONT_MONO, FONT_SOURCE = register_fonts()


def ascii_punctuation(value: str) -> str:
    """Normalize typographic punctuation; PDF prose uses ASCII hyphens."""

    translations = {
        ord("\u2010"): "-",
        ord("\u2011"): "-",
        ord("\u2012"): "-",
        ord("\u2013"): "-",
        ord("\u2014"): "-",
        ord("\u2212"): "-",
        ord("\u2018"): "'",
        ord("\u2019"): "'",
        ord("\u201c"): '"',
        ord("\u201d"): '"',
        ord("\u00a0"): " ",
        ord("\u2026"): "...",
    }
    return value.translate(translations)


def inline_markup(value: str) -> str:
    """Convert the small inline Markdown subset used in the paper."""

    value = ascii_punctuation(value.strip())
    code_tokens: list[str] = []

    def stash_code(match: re.Match[str]) -> str:
        token = f"@@KENDRCODE{len(code_tokens)}@@"
        code_tokens.append(html.escape(match.group(1)))
        return token

    value = re.sub(r"`([^`]+)`", stash_code, value)
    value = html.escape(value, quote=False)

    def replace_link(match: re.Match[str]) -> str:
        label = match.group(1)
        url = match.group(2)
        return f'<a href="{html.escape(url, quote=True)}" color="#B8551A"><u>{label}</u></a>'

    value = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", replace_link, value)
    value = re.sub(
        r"&lt;(https?://[^&]+)&gt;",
        lambda match: (
            f'<a href="{html.escape(html.unescape(match.group(1)), quote=True)}" '
            f'color="#B8551A"><u>{match.group(1)}</u></a>'
        ),
        value,
    )
    value = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", value)
    for index, code in enumerate(code_tokens):
        value = value.replace(
            f"@@KENDRCODE{index}@@",
            f'<font name="{FONT_MONO}" color="#2B2925">{code}</font>',
        )
    return value


def make_styles() -> dict[str, ParagraphStyle]:
    base = getSampleStyleSheet()
    return {
        "cover_title": ParagraphStyle(
            "CoverTitle",
            parent=base["Title"],
            fontName=FONT_HEADING,
            fontSize=34,
            leading=38,
            textColor=colors.white,
            alignment=TA_LEFT,
            spaceAfter=8,
        ),
        "cover_subtitle": ParagraphStyle(
            "CoverSubtitle",
            parent=base["Normal"],
            fontName=FONT,
            fontSize=15,
            leading=21,
            textColor=DARK_TEXT,
            spaceAfter=18,
        ),
        "cover_meta": ParagraphStyle(
            "CoverMeta",
            parent=base["Normal"],
            fontName=FONT,
            fontSize=9.5,
            leading=14,
            textColor=DARK_SECONDARY,
        ),
        "h1": ParagraphStyle(
            "WP-H1",
            parent=base["Heading1"],
            fontName=FONT_HEADING,
            fontSize=22,
            leading=27,
            textColor=NAVY,
            spaceBefore=2,
            spaceAfter=12,
            keepWithNext=True,
        ),
        "h2": ParagraphStyle(
            "WP-H2",
            parent=base["Heading2"],
            fontName=FONT_HEADING,
            fontSize=16,
            leading=20,
            textColor=NAVY,
            spaceBefore=9,
            spaceAfter=8,
            keepWithNext=True,
        ),
        "h3": ParagraphStyle(
            "WP-H3",
            parent=base["Heading3"],
            fontName=FONT_HEADING,
            fontSize=11.5,
            leading=15,
            textColor=BLUE,
            spaceBefore=9,
            spaceAfter=5,
            keepWithNext=True,
        ),
        "h4": ParagraphStyle(
            "WP-H4",
            parent=base["Heading4"],
            fontName=FONT_BOLD,
            fontSize=9.6,
            leading=12.5,
            textColor=INK,
            spaceBefore=7,
            spaceAfter=4,
            keepWithNext=True,
        ),
        "body": ParagraphStyle(
            "WP-Body",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=9.2,
            leading=13.15,
            textColor=INK,
            spaceAfter=6,
            alignment=TA_LEFT,
            splitLongWords=True,
        ),
        "small": ParagraphStyle(
            "WP-Small",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.5,
            leading=10.2,
            textColor=INK,
            splitLongWords=True,
        ),
        "table_header": ParagraphStyle(
            "WP-TableHeader",
            parent=base["BodyText"],
            fontName=FONT_HEADING,
            fontSize=7.0,
            leading=9.0,
            textColor=colors.white,
            splitLongWords=True,
        ),
        "table_body": ParagraphStyle(
            "WP-TableBody",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=6.9,
            leading=8.9,
            textColor=INK,
            splitLongWords=True,
        ),
        "code": ParagraphStyle(
            "WP-Code",
            parent=base["Code"],
            fontName=FONT_MONO,
            fontSize=6.9,
            leading=9.2,
            textColor=INK,
            leftIndent=7,
            rightIndent=7,
            borderColor=GRID,
            borderWidth=0.6,
            borderPadding=7,
            backColor=SURFACE,
            spaceBefore=3,
            spaceAfter=8,
        ),
        "quote": ParagraphStyle(
            "WP-Quote",
            parent=base["BodyText"],
            fontName=FONT_ITALIC,
            fontSize=9.3,
            leading=13.5,
            textColor=INK,
            leftIndent=12,
            rightIndent=8,
            borderColor=BLUE,
            borderWidth=0,
            borderPadding=8,
            backColor=LIGHT_BLUE,
            spaceBefore=4,
            spaceAfter=9,
        ),
        "figure_caption": ParagraphStyle(
            "WP-FigureCaption",
            parent=base["BodyText"],
            fontName=FONT_ITALIC,
            fontSize=7.5,
            leading=10,
            textColor=MUTED,
            alignment=TA_CENTER,
            spaceBefore=3,
            spaceAfter=9,
        ),
        "toc_title": ParagraphStyle(
            "WP-TOCTitle",
            parent=base["Heading1"],
            fontName=FONT_HEADING,
            fontSize=22,
            leading=26,
            textColor=NAVY,
            spaceAfter=14,
        ),
    }


STYLES = make_styles()


class DeterministicCanvas(pdf_canvas.Canvas):
    """Remove wall-clock metadata so identical sources build identical PDFs."""

    def __init__(self, *args: object, **kwargs: object) -> None:
        kwargs["invariant"] = 1
        super().__init__(*args, **kwargs)


class WhitepaperDocTemplate(BaseDocTemplate):
    """Document template with cover, body header/footer, TOC, and bookmarks."""

    def __init__(self, filename: str, **kwargs: object) -> None:
        super().__init__(filename, pagesize=A4, **kwargs)
        cover_frame = Frame(
            MARGIN_X,
            MARGIN_BOTTOM,
            CONTENT_WIDTH,
            PAGE_HEIGHT - MARGIN_TOP - MARGIN_BOTTOM,
            id="cover-frame",
            leftPadding=0,
            rightPadding=0,
            topPadding=0,
            bottomPadding=0,
        )
        body_frame = Frame(
            MARGIN_X,
            MARGIN_BOTTOM,
            CONTENT_WIDTH,
            PAGE_HEIGHT - MARGIN_TOP - MARGIN_BOTTOM,
            id="body-frame",
            leftPadding=0,
            rightPadding=0,
            topPadding=0,
            bottomPadding=0,
        )
        self.addPageTemplates(
            [
                PageTemplate(id="Cover", frames=[cover_frame], onPage=draw_cover_page),
                PageTemplate(id="Body", frames=[body_frame], onPage=draw_body_page),
            ]
        )
        self._bookmark_index = 0

    def beforeDocument(self) -> None:  # noqa: N802
        # ``multiBuild`` performs several complete layout passes so the table
        # of contents can settle. Bookmark keys must be identical each pass.
        self._bookmark_index = 0

    def afterFlowable(self, flowable: Flowable) -> None:  # noqa: N802
        if not isinstance(flowable, Paragraph):
            return
        style_name = flowable.style.name
        if style_name not in {"WP-H1", "WP-H2", "WP-H3", "WP-H4"}:
            return
        level = {"WP-H1": 0, "WP-H2": 0, "WP-H3": 1, "WP-H4": 2}[style_name]
        text = flowable.getPlainText()
        key = f"heading-{self._bookmark_index}"
        self._bookmark_index += 1
        self.canv.bookmarkPage(key)
        self.canv.addOutlineEntry(text, key, level=level, closed=False)
        self.notify("TOCEntry", (level, text, self.page, key))


def draw_cover_page(canvas: object, _doc: object) -> None:
    canvas.saveState()
    canvas.setFillColor(NAVY)
    canvas.rect(0, 0, PAGE_WIDTH, PAGE_HEIGHT, stroke=0, fill=1)
    canvas.setFillColor(CYAN)
    canvas.rect(0, PAGE_HEIGHT - 6 * mm, PAGE_WIDTH, 6 * mm, stroke=0, fill=1)
    canvas.rect(0, 0, 2.5 * mm, PAGE_HEIGHT, stroke=0, fill=1)
    canvas.setStrokeColor(HexColor("#2B2925"))
    canvas.setLineWidth(0.5)
    for index in range(8):
        y = 18 * mm + index * 14 * mm
        canvas.line(20 * mm, y, PAGE_WIDTH - 20 * mm, y)
    canvas.drawImage(
        str(BRAND_MARK),
        PAGE_WIDTH - 55 * mm,
        PAGE_HEIGHT - 59 * mm,
        width=37 * mm,
        height=37 * mm,
        mask="auto",
        preserveAspectRatio=True,
        anchor="c",
    )
    canvas.restoreState()


def draw_body_page(canvas: object, doc: BaseDocTemplate) -> None:
    canvas.saveState()
    canvas.setFillColor(PAPER)
    canvas.rect(0, 0, PAGE_WIDTH, PAGE_HEIGHT, stroke=0, fill=1)
    canvas.setStrokeColor(GRID)
    canvas.setLineWidth(0.5)
    canvas.line(MARGIN_X, PAGE_HEIGHT - 13 * mm, PAGE_WIDTH - MARGIN_X, PAGE_HEIGHT - 13 * mm)
    canvas.drawImage(
        str(BRAND_ICON),
        MARGIN_X,
        PAGE_HEIGHT - 11.2 * mm,
        width=5.2 * mm,
        height=5.2 * mm,
        mask="auto",
        preserveAspectRatio=True,
        anchor="c",
    )
    canvas.setFont(FONT_BOLD, 7.2)
    canvas.setFillColor(NAVY)
    canvas.drawString(MARGIN_X + 7 * mm, PAGE_HEIGHT - 10 * mm, "KENDR OPTIMIZER")
    canvas.setFont(FONT, 7.2)
    canvas.setFillColor(MUTED)
    canvas.drawRightString(
        PAGE_WIDTH - MARGIN_X,
        PAGE_HEIGHT - 10 * mm,
        "Verification-Gated Typed Token Reduction",
    )
    canvas.setStrokeColor(CYAN)
    canvas.setLineWidth(1.2)
    canvas.line(MARGIN_X, PAGE_HEIGHT - 13 * mm, MARGIN_X + 21 * mm, PAGE_HEIGHT - 13 * mm)
    canvas.setStrokeColor(GRID)
    canvas.setLineWidth(0.5)
    canvas.line(MARGIN_X, 12 * mm, PAGE_WIDTH - MARGIN_X, 12 * mm)
    canvas.setFont(FONT, 7.2)
    canvas.drawString(MARGIN_X, 8 * mm, "Technical whitepaper v0.1 - August 2026")
    canvas.drawRightString(PAGE_WIDTH - MARGIN_X, 8 * mm, str(doc.page))
    canvas.restoreState()


class SectionRule(Flowable):
    def __init__(self) -> None:
        super().__init__()
        self.width = CONTENT_WIDTH
        self.height = 5

    def draw(self) -> None:
        self.canv.setStrokeColor(BLUE)
        self.canv.setLineWidth(1.5)
        self.canv.line(0, 3, 26 * mm, 3)
        self.canv.setStrokeColor(GRID)
        self.canv.setLineWidth(0.5)
        self.canv.line(27 * mm, 3, self.width, 3)


def arrow(drawing: Drawing, x1: float, y1: float, x2: float, y2: float) -> None:
    drawing.add(Line(x1, y1, x2, y2, strokeColor=BLUE, strokeWidth=1.5))
    drawing.add(Line(x2, y2, x2 - 5, y2 + 3, strokeColor=BLUE, strokeWidth=1.5))
    drawing.add(Line(x2, y2, x2 - 5, y2 - 3, strokeColor=BLUE, strokeWidth=1.5))


def system_boundary_figure() -> Drawing:
    width, height = CONTENT_WIDTH, 90 * mm
    drawing = Drawing(width, height)
    drawing.add(Rect(0, 0, width, height, rx=8, ry=8, fillColor=PAPER, strokeColor=GRID))
    box_w, box_h = 42 * mm, 26 * mm
    y = 34 * mm
    xs = [8 * mm, 64 * mm, 120 * mm]
    fills = [LIGHT_BLUE, LIGHT_GREEN, SURFACE]
    titles = ["Host / agent", "Kendr Optimizer", "Any LLM"]
    subtitles = ["owns provider request", "transform only", "selected by host"]
    for x, fill, title, subtitle in zip(xs, fills, titles, subtitles, strict=True):
        drawing.add(Rect(x, y, box_w, box_h, rx=5, ry=5, fillColor=fill, strokeColor=GRID))
        drawing.add(String(x + box_w / 2, y + 16 * mm, title, textAnchor="middle", fontName=FONT_BOLD, fontSize=10, fillColor=NAVY))
        drawing.add(String(x + box_w / 2, y + 9 * mm, subtitle, textAnchor="middle", fontName=FONT, fontSize=7.5, fillColor=MUTED))
    arrow(drawing, xs[0] + box_w, y + box_h / 2, xs[1] - 3 * mm, y + box_h / 2)
    arrow(drawing, xs[1] + box_w, y + box_h / 2, xs[2] - 3 * mm, y + box_h / 2)
    drawing.add(String(57 * mm, y + box_h / 2 + 5 * mm, "typed envelope", textAnchor="middle", fontName=FONT, fontSize=6.8, fillColor=MUTED))
    drawing.add(String(113 * mm, y + box_h / 2 + 5 * mm, "provider request", textAnchor="middle", fontName=FONT, fontSize=6.8, fillColor=MUTED))
    drawing.add(String(width / 2, 18 * mm, "validate -> propose -> measure -> verify -> accept or revert", textAnchor="middle", fontName=FONT_BOLD, fontSize=9, fillColor=GREEN))
    drawing.add(String(width / 2, 9 * mm, "No provider credentials. No model routing. No outbound model call.", textAnchor="middle", fontName=FONT, fontSize=8, fillColor=INK))
    return drawing


def risk_ladder_figure() -> Drawing:
    width, height = CONTENT_WIDTH, 54 * mm
    drawing = Drawing(width, height)
    drawing.add(Rect(0, 0, width, height, rx=8, ry=8, fillColor=PAPER, strokeColor=GRID))
    labels = ["Pass-through", "Representation-safe", "Recoverable", "Extractive", "Learned"]
    colors_fill = [SURFACE, LIGHT_BLUE, LIGHT_GREEN, LIGHT_AMBER, LIGHT_RED]
    x = 7 * mm
    gap = 3 * mm
    box_w = (width - 14 * mm - 4 * gap) / 5
    for index, (label, fill) in enumerate(zip(labels, colors_fill, strict=True)):
        box_x = x + index * (box_w + gap)
        drawing.add(Rect(box_x, 17 * mm, box_w, 21 * mm, rx=4, ry=4, fillColor=fill, strokeColor=GRID))
        drawing.add(String(box_x + box_w / 2, 29 * mm, f"Q{index}", textAnchor="middle", fontName=FONT_BOLD, fontSize=10, fillColor=NAVY))
        drawing.add(String(box_x + box_w / 2, 22 * mm, label, textAnchor="middle", fontName=FONT, fontSize=6.6, fillColor=INK))
    drawing.add(String(width / 2, 8 * mm, "Higher risk requires explicit policy and stronger downstream evidence", textAnchor="middle", fontName=FONT_ITALIC, fontSize=8, fillColor=MUTED))
    return drawing


def evidence_ladder_figure() -> Drawing:
    width, height = CONTENT_WIDTH, 68 * mm
    drawing = Drawing(width, height)
    drawing.add(Rect(0, 0, width, height, rx=8, ry=8, fillColor=PAPER, strokeColor=GRID))
    labels = [
        ("E0", "Bytes"),
        ("E1", "Local tokens"),
        ("E2", "Estimated cost"),
        ("E3", "Observed run"),
        ("E4", "Paired usage"),
        ("E5", "Quality-adjusted"),
    ]
    fills = [SURFACE, LIGHT_BLUE, LIGHT_AMBER, LIGHT_RED, HexColor("#EEE5DA"), LIGHT_GREEN]
    step_w = 25 * mm
    start_x = 8 * mm
    for index, ((level, label), fill) in enumerate(zip(labels, fills, strict=True)):
        x = start_x + index * 27 * mm
        y = 9 * mm + index * 5.2 * mm
        drawing.add(Rect(x, y, step_w, 18 * mm, rx=3, ry=3, fillColor=fill, strokeColor=GRID))
        drawing.add(String(x + step_w / 2, y + 11 * mm, level, textAnchor="middle", fontName=FONT_BOLD, fontSize=9, fillColor=NAVY))
        drawing.add(String(x + step_w / 2, y + 5 * mm, label, textAnchor="middle", fontName=FONT, fontSize=6.2, fillColor=INK))
    drawing.add(String(width / 2, 57 * mm, "Claim strength rises only when the required observation exists", textAnchor="middle", fontName=FONT_BOLD, fontSize=9, fillColor=GREEN))
    return drawing


def qualification_pipeline_figure() -> Drawing:
    """Show exactly when a raw result becomes eligible for primary ranking."""

    width, height = CONTENT_WIDTH, 98 * mm
    drawing = Drawing(width, height)
    drawing.add(Rect(0, 0, width, height, rx=8, ry=8, fillColor=SURFACE, strokeColor=GRID))
    drawing.add(
        String(
            7 * mm,
            height - 9 * mm,
            "One configuration, one surface, one reproducible qualification decision",
            fontName=FONT_BOLD,
            fontSize=9.5,
            fillColor=NAVY,
        )
    )

    box_w, box_h = 46 * mm, 19 * mm
    top_y = 57 * mm
    xs = [7 * mm, 64 * mm, 121 * mm]
    boxes = [
        ("1. Freeze the track", "5 prompt or 4 tool cases", "plus versions and settings"),
        ("2. Execute and retain", "input, output, status", "stdout, stderr, environment"),
        ("3. Recount and aggregate", "complete visible strings", "signed o200k_base delta"),
    ]
    fills = [LIGHT_BLUE, LIGHT_AMBER, LIGHT_GREEN]
    for x, (title, line_one, line_two), fill in zip(xs, boxes, fills, strict=True):
        drawing.add(Rect(x, top_y, box_w, box_h, rx=4, ry=4, fillColor=fill, strokeColor=GRID))
        drawing.add(String(x + box_w / 2, top_y + 12.5 * mm, title, textAnchor="middle", fontName=FONT_BOLD, fontSize=7.4, fillColor=NAVY))
        drawing.add(String(x + box_w / 2, top_y + 7 * mm, line_one, textAnchor="middle", fontName=FONT, fontSize=6.4, fillColor=INK))
        drawing.add(String(x + box_w / 2, top_y + 3.2 * mm, line_two, textAnchor="middle", fontName=FONT, fontSize=6.2, fillColor=MUTED))
    arrow(drawing, xs[0] + box_w, top_y + box_h / 2, xs[1] - 3 * mm, top_y + box_h / 2)
    arrow(drawing, xs[1] + box_w, top_y + box_h / 2, xs[2] - 3 * mm, top_y + box_h / 2)

    gate_x, gate_y, gate_w, gate_h = 44 * mm, 29 * mm, 86 * mm, 18 * mm
    source_x = xs[2] + box_w / 2
    gate_center = gate_x + gate_w / 2
    drawing.add(Line(source_x, top_y, source_x, gate_y + gate_h + 5 * mm, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Line(source_x, gate_y + gate_h + 5 * mm, gate_center, gate_y + gate_h + 5 * mm, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Line(gate_center, gate_y + gate_h + 5 * mm, gate_center, gate_y + gate_h, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Line(gate_center, gate_y + gate_h, gate_center - 3, gate_y + gate_h + 5, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Line(gate_center, gate_y + gate_h, gate_center + 3, gate_y + gate_h + 5, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Rect(gate_x, gate_y, gate_w, gate_h, rx=4, ry=4, fillColor=LIGHT_AMBER, strokeColor=CYAN, strokeWidth=1))
    drawing.add(String(gate_center, gate_y + 11.7 * mm, "4. Apply the public full-surface gate", textAnchor="middle", fontName=FONT_BOLD, fontSize=7.7, fillColor=NAVY))
    drawing.add(String(gate_center, gate_y + 6.4 * mm, "5/5 or 4/4 completed; zero failures; every case gate passed", textAnchor="middle", fontName=FONT, fontSize=6.4, fillColor=INK))
    drawing.add(String(gate_center, gate_y + 2.8 * mm, "required literals + JSON equality when required + exact query marker", textAnchor="middle", fontName=FONT, fontSize=5.9, fillColor=MUTED))

    left_x, right_x = 8 * mm, 96 * mm
    outcome_y, outcome_w, outcome_h = 4 * mm, 70 * mm, 14 * mm
    left_center, right_center = left_x + outcome_w / 2, right_x + outcome_w / 2
    split_y = gate_y - 5 * mm
    drawing.add(Line(gate_center, gate_y, gate_center, split_y, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(Line(left_center, split_y, right_center, split_y, strokeColor=BLUE, strokeWidth=1.4))
    for center in (left_center, right_center):
        drawing.add(Line(center, split_y, center, outcome_y + outcome_h, strokeColor=BLUE, strokeWidth=1.4))
        drawing.add(Line(center, outcome_y + outcome_h, center - 3, outcome_y + outcome_h + 5, strokeColor=BLUE, strokeWidth=1.4))
        drawing.add(Line(center, outcome_y + outcome_h, center + 3, outcome_y + outcome_h + 5, strokeColor=BLUE, strokeWidth=1.4))
    drawing.add(String(left_center, split_y + 1.5 * mm, "gate fails", textAnchor="middle", fontName=FONT_BOLD, fontSize=5.8, fillColor=RED))
    drawing.add(String(right_center, split_y + 1.5 * mm, "gate passes", textAnchor="middle", fontName=FONT_BOLD, fontSize=5.8, fillColor=GREEN))
    drawing.add(Rect(left_x, outcome_y, outcome_w, outcome_h, rx=4, ry=4, fillColor=LIGHT_RED, strokeColor=RED))
    drawing.add(String(left_center, outcome_y + 8.5 * mm, "QUALIFIED REDUCTION = N/A", textAnchor="middle", fontName=FONT_BOLD, fontSize=7.2, fillColor=RED))
    drawing.add(String(left_center, outcome_y + 3.4 * mm, "raw stays diagnostic; row receives no primary rank", textAnchor="middle", fontName=FONT, fontSize=5.9, fillColor=INK))
    drawing.add(Rect(right_x, outcome_y, outcome_w, outcome_h, rx=4, ry=4, fillColor=LIGHT_GREEN, strokeColor=GREEN))
    drawing.add(String(right_center, outcome_y + 8.5 * mm, "QUALIFIED REDUCTION = RAW", textAnchor="middle", fontName=FONT_BOLD, fontSize=7.2, fillColor=GREEN))
    drawing.add(String(right_center, outcome_y + 3.4 * mm, "unchanged percentage enters the primary ranking", textAnchor="middle", fontName=FONT, fontSize=5.9, fillColor=INK))
    return drawing


def ranking_chart(
    title: str,
    rows: list[tuple[str, float, float | None, str]],
    note: str,
) -> Drawing:
    width = CONTENT_WIDTH
    row_h = 14 * mm
    height = 24 * mm + len(rows) * row_h
    drawing = Drawing(width, height)
    drawing.add(Rect(0, 0, width, height, rx=8, ry=8, fillColor=PAPER, strokeColor=GRID))
    drawing.add(String(7 * mm, height - 10 * mm, title, fontName=FONT_BOLD, fontSize=9.5, fillColor=NAVY))
    chart_x = 52 * mm
    chart_w = width - chart_x - 10 * mm
    max_value = 100.0
    for tick in (0, 25, 50, 75, 100):
        x = chart_x + chart_w * tick / max_value
        drawing.add(Line(x, 12 * mm, x, height - 16 * mm, strokeColor=GRID, strokeWidth=0.4))
        drawing.add(String(x, 8 * mm, str(tick), textAnchor="middle", fontName=FONT, fontSize=6, fillColor=MUTED))
    for index, (name, raw, qualified, gate_status) in enumerate(rows):
        y = height - 22 * mm - index * row_h
        drawing.add(String(6 * mm, y + 2.2 * mm, name, fontName=FONT, fontSize=7.2, fillColor=INK))
        raw_w = chart_w * raw / max_value
        drawing.add(Rect(chart_x, y, raw_w, 5.5 * mm, fillColor=GRID, strokeColor=None))
        if qualified is not None:
            qualified_w = chart_w * qualified / max_value
            drawing.add(Rect(chart_x, y, qualified_w, 5.5 * mm, fillColor=GREEN, strokeColor=None))
            label = f"{qualified:.2f}% qualified"
            label_color = colors.white if qualified_w > 35 * mm else GREEN
            label_x = chart_x + max(qualified_w - 2 * mm, qualified_w + 2 * mm)
            anchor = "end" if qualified_w > 35 * mm else "start"
            drawing.add(String(label_x, y + 1.6 * mm, label, textAnchor=anchor, fontName=FONT_BOLD, fontSize=6.5, fillColor=label_color))
            status = f"{gate_status}; admitted to primary ranking"
            status_color = GREEN
        else:
            label_color = INK if raw_w < 34 * mm else colors.white
            label_x = chart_x + raw_w + 2 * mm if raw_w < 34 * mm else chart_x + raw_w - 2 * mm
            anchor = "start" if raw_w < 34 * mm else "end"
            drawing.add(String(label_x, y + 1.6 * mm, f"{raw:.2f}% raw", textAnchor=anchor, fontName=FONT_BOLD, fontSize=6.3, fillColor=label_color))
            status = f"{gate_status} -> qualified N/A; excluded"
            status_color = RED
        drawing.add(String(chart_x, y - 3.1 * mm, status, fontName=FONT, fontSize=5.7, fillColor=status_color))
    drawing.add(
        String(
            width - 7 * mm,
            height - 10 * mm,
            note,
            textAnchor="end",
            fontName=FONT_ITALIC,
            fontSize=6.2,
            fillColor=MUTED,
        )
    )
    return drawing


def figure_for(name: str) -> list[Flowable]:
    if name == "system-boundary":
        drawing = system_boundary_figure()
        caption = "Figure 1. Kendr's transform-only trust boundary."
    elif name == "risk-ladder":
        drawing = risk_ladder_figure()
        caption = "Figure 2. Ordered risk classes; the default ceiling is recoverable."
    elif name == "evidence-ladder":
        drawing = evidence_ladder_figure()
        caption = "Figure 3. Local token reduction and provider-verified savings are different evidence levels."
    elif name == "qualification-pipeline":
        drawing = qualification_pipeline_figure()
        caption = "Figure 4. Qualification is a binary admission gate: it preserves or withholds the raw percentage; it never discounts it."
    elif name == "prompt-ranking":
        drawing = ranking_chart(
            "Prompt/context primary ranking",
            [
                ("Kendr default", 71.64, 71.64, "5/5 fixture-gate cases passed"),
                ("LLMLingua GPT-2", 64.44, 64.44, "5/5 fixture-gate cases passed"),
                ("Headroom structural", 38.57, 38.57, "5/5 fixture-gate cases passed"),
                ("OmniRoute stack", 1.54, 1.54, "5/5 fixture-gate cases passed"),
                ("Headroom structural default", 0.00, 0.00, "5/5 fixture-gate cases passed"),
            ],
            "Authored 5-case track; full coverage required",
        )
        caption = "Figure 5. Every shown row completed all five cases and passed all five composite fixture gates."
    elif name == "tool-ranking":
        drawing = ranking_chart(
            "Command/tool-output raw versus qualified reduction",
            [
                ("RTK", 97.27, None, "0/4 fixture-gate cases passed"),
                ("OmniRoute stack", 71.45, None, "1/4 fixture-gate cases passed"),
                ("Kendr default", 61.38, 61.38, "4/4 fixture-gate cases passed"),
                ("Headroom Kompress", 21.58, None, "2/4 fixture-gate cases passed"),
                ("Headroom structural", 17.26, None, "3/4 fixture-gate cases passed"),
                ("Headroom structural target-50", 17.26, None, "3/4 fixture-gate cases passed"),
            ],
            "Authored 4-case track; raw stays diagnostic",
        )
        caption = "Figure 6. A row below 4/4 composite fixture-gate passes receives qualified N/A and no primary rank."
    else:
        raise ValueError(f"unknown PDF figure marker: {name}")
    return [Spacer(1, 3 * mm), drawing, Paragraph(caption, STYLES["figure_caption"])]


def split_table_row(line: str) -> list[str]:
    value = line.strip()
    if value.startswith("|"):
        value = value[1:]
    if value.endswith("|"):
        value = value[:-1]
    return [cell.strip() for cell in value.split("|")]


def table_flowable(lines: list[str]) -> Table:
    rows = [split_table_row(line) for line in lines]
    if len(rows) < 2:
        raise ValueError("Markdown table requires a header and delimiter")
    delimiter = rows[1]
    if not all(re.fullmatch(r":?-{3,}:?", cell.replace(" ", "")) for cell in delimiter):
        raise ValueError(f"invalid Markdown table delimiter: {lines[1]}")
    rows = [rows[0], *rows[2:]]
    column_count = len(rows[0])
    if any(len(row) != column_count for row in rows):
        raise ValueError("Markdown table has inconsistent columns")

    max_lengths = [
        max(8, min(42, max(len(ascii_punctuation(row[index])) for row in rows)))
        for index in range(column_count)
    ]
    total_weight = sum(max_lengths)
    widths = [CONTENT_WIDTH * weight / total_weight for weight in max_lengths]
    min_width = 15 * mm if column_count <= 5 else 10 * mm
    widths = [max(min_width, width) for width in widths]
    scale = CONTENT_WIDTH / sum(widths)
    widths = [width * scale for width in widths]

    cells: list[list[Paragraph]] = []
    for row_index, row in enumerate(rows):
        style = STYLES["table_header"] if row_index == 0 else STYLES["table_body"]
        cells.append([Paragraph(inline_markup(cell), style) for cell in row])

    table = Table(cells, colWidths=widths, repeatRows=1, hAlign="LEFT")
    commands: list[tuple[object, ...]] = [
        ("BACKGROUND", (0, 0), (-1, 0), NAVY),
        ("TEXTCOLOR", (0, 0), (-1, 0), colors.white),
        ("GRID", (0, 0), (-1, -1), 0.35, GRID),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 4),
        ("RIGHTPADDING", (0, 0), (-1, -1), 4),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
    ]
    for row_index in range(1, len(rows)):
        if row_index % 2 == 0:
            commands.append(("BACKGROUND", (0, row_index), (-1, row_index), PAPER))
    table.setStyle(TableStyle(commands))
    return table


def wrap_code_line(line: str, width: int = 104) -> list[str]:
    if len(line) <= width:
        return [line]
    indent = re.match(r"\s*", line).group(0)
    continuation = indent + "  "
    output: list[str] = []
    remaining = line
    while len(remaining) > width:
        cut = max(
            remaining.rfind(" ", 0, width),
            remaining.rfind(",", 0, width),
            remaining.rfind("/", 0, width),
        )
        if cut <= len(indent) + 8:
            cut = width
        output.append(remaining[:cut].rstrip())
        remaining = continuation + remaining[cut:].lstrip()
    output.append(remaining)
    return output


def code_flowable(lines: Iterable[str]) -> XPreformatted:
    normalized: list[str] = []
    for line in lines:
        normalized.extend(wrap_code_line(ascii_punctuation(line.expandtabs(4))))
    escaped = html.escape("\n".join(normalized))
    return XPreformatted(escaped, STYLES["code"])


def is_table_delimiter(line: str) -> bool:
    if not line.strip().startswith("|"):
        return False
    cells = split_table_row(line)
    return bool(cells) and all(
        re.fullmatch(r":?-{3,}:?", cell.replace(" ", "")) for cell in cells
    )


def parse_markdown(source: str) -> list[Flowable]:
    lines = source.splitlines()
    start = next(
        (index for index, line in enumerate(lines) if line.strip() == "## Abstract"),
        0,
    )
    lines = lines[start:]
    story: list[Flowable] = []
    index = 0
    first_body_heading = True
    section_count = 0

    def paragraph_from(buffer: list[str], style: str = "body") -> None:
        text = " ".join(line.strip() for line in buffer).strip()
        if text:
            story.append(Paragraph(inline_markup(text), STYLES[style]))

    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue

        figure_match = re.fullmatch(r"<!--\s*pdf-figure:([a-z0-9-]+)\s*-->", stripped)
        if figure_match:
            story.extend(figure_for(figure_match.group(1)))
            index += 1
            continue

        if stripped.startswith("```"):
            fence = stripped[:3]
            index += 1
            code_lines: list[str] = []
            while index < len(lines) and not lines[index].strip().startswith(fence):
                code_lines.append(lines[index])
                index += 1
            if index >= len(lines):
                raise ValueError("unterminated Markdown code fence")
            index += 1
            story.append(code_flowable(code_lines))
            continue

        heading = re.match(r"^(#{1,4})\s+(.+)$", stripped)
        if heading:
            level = len(heading.group(1))
            title = ascii_punctuation(heading.group(2))
            if level == 2:
                numbered = bool(re.match(r"\d+\.\s", title))
                appendix = title.startswith("Appendix ")
                reference = title == "References"
                if not first_body_heading and (numbered or appendix or reference):
                    # Prefer a fresh section page when the preceding section has
                    # consumed most of the sheet, but never create an orphaned
                    # spill page followed by a forced break.
                    minimum_section_space = 125 * mm if appendix else 75 * mm
                    story.append(CondPageBreak(minimum_section_space))
                first_body_heading = False
                section_count += 1
                story.append(Paragraph(inline_markup(title), STYLES["h2"]))
                story.append(SectionRule())
            elif level == 3:
                story.append(Paragraph(inline_markup(title), STYLES["h3"]))
            elif level == 4:
                story.append(Paragraph(inline_markup(title), STYLES["h4"]))
            else:
                story.append(Paragraph(inline_markup(title), STYLES["h1"]))
            index += 1
            continue

        if stripped.startswith(">"):
            quote_lines: list[str] = []
            while index < len(lines) and lines[index].strip().startswith(">"):
                quote_lines.append(lines[index].strip()[1:].strip())
                index += 1
            paragraph_from(quote_lines, "quote")
            continue

        if (
            stripped.startswith("|")
            and index + 1 < len(lines)
            and is_table_delimiter(lines[index + 1])
        ):
            table_lines = [line, lines[index + 1]]
            index += 2
            while index < len(lines) and lines[index].strip().startswith("|"):
                table_lines.append(lines[index])
                index += 1
            story.append(table_flowable(table_lines))
            story.append(Spacer(1, 4 * mm))
            continue

        bullet_match = re.match(r"^\s*[-*]\s+(.+)$", line)
        numbered_match = re.match(r"^\s*(\d+)\.\s+(.+)$", line)
        if bullet_match or numbered_match:
            ordered = numbered_match is not None
            items: list[ListItem] = []
            while index < len(lines):
                current = lines[index]
                match = (
                    re.match(r"^\s*(\d+)\.\s+(.+)$", current)
                    if ordered
                    else re.match(r"^\s*[-*]\s+(.+)$", current)
                )
                if not match:
                    break
                content = match.group(2) if ordered else match.group(1)
                index += 1
                continuation: list[str] = [content]
                while (
                    index < len(lines)
                    and lines[index].strip()
                    and not re.match(r"^\s*(?:[-*]|\d+\.)\s+", lines[index])
                    and not re.match(r"^#{1,4}\s+", lines[index].strip())
                    and not lines[index].strip().startswith(("|", "```", ">", "<!--"))
                ):
                    continuation.append(lines[index].strip())
                    index += 1
                item_para = Paragraph(inline_markup(" ".join(continuation)), STYLES["body"])
                items.append(ListItem(item_para, leftIndent=12, bulletColor=BLUE))
            list_options = {
                "bulletType": "1" if ordered else "bullet",
                "leftIndent": 16,
                "bulletFontName": FONT_BOLD,
                "bulletFontSize": 7.5,
                "bulletColor": BLUE,
                "spaceAfter": 5,
            }
            if ordered:
                list_options["start"] = "1"
            story.append(ListFlowable(items, **list_options))
            continue

        if stripped in {"---", "***"}:
            story.append(HRFlowable(width="100%", thickness=0.5, color=GRID, spaceBefore=5, spaceAfter=7))
            index += 1
            continue

        buffer = [line]
        index += 1
        while index < len(lines):
            candidate = lines[index]
            candidate_stripped = candidate.strip()
            if not candidate_stripped:
                break
            if (
                re.match(r"^#{1,4}\s+", candidate_stripped)
                or candidate_stripped.startswith(("```", ">", "<!--"))
                or re.match(r"^\s*[-*]\s+", candidate)
                or re.match(r"^\s*\d+\.\s+", candidate)
                or (
                    candidate_stripped.startswith("|")
                    and index + 1 < len(lines)
                    and is_table_delimiter(lines[index + 1])
                )
            ):
                break
            buffer.append(candidate)
            index += 1
        paragraph_from(buffer)

    if section_count < 20:
        raise ValueError("whitepaper parser found unexpectedly few top-level sections")
    return story


def cover_story(source_sha256: str) -> list[Flowable]:
    metrics = Table(
        [
            [
                Paragraph("<b>9</b><br/><font size=8>native engines</font>", STYLES["cover_meta"]),
                Paragraph("<b>7</b><br/><font size=8>audited harnesses</font>", STYLES["cover_meta"]),
                Paragraph("<b>2</b><br/><font size=8>exact BPE profiles</font>", STYLES["cover_meta"]),
            ]
        ],
        colWidths=[46 * mm, 46 * mm, 46 * mm],
        rowHeights=[20 * mm],
    )
    metrics.setStyle(
        TableStyle(
            [
                ("BACKGROUND", (0, 0), (-1, -1), DARK_SURFACE),
                ("BOX", (0, 0), (-1, -1), 0.5, DARK_RAISED),
                ("INNERGRID", (0, 0), (-1, -1), 0.5, DARK_RAISED),
                ("VALIGN", (0, 0), (-1, -1), "MIDDLE"),
                ("ALIGN", (0, 0), (-1, -1), "CENTER"),
            ]
        )
    )
    toc = TableOfContents()
    toc.levelStyles = [
        ParagraphStyle(
            "TOC-Level-0",
            fontName=FONT,
            fontSize=9.5,
            leading=14,
            textColor=INK,
            leftIndent=0,
            firstLineIndent=0,
            spaceBefore=2,
        ),
        ParagraphStyle(
            "TOC-Level-1",
            fontName=FONT,
            fontSize=8.2,
            leading=11,
            textColor=MUTED,
            leftIndent=10 * mm,
            firstLineIndent=0,
        ),
        ParagraphStyle(
            "TOC-Level-2",
            fontName=FONT,
            fontSize=7.6,
            leading=10,
            textColor=MUTED,
            leftIndent=18 * mm,
            firstLineIndent=0,
        ),
    ]
    return [
        Spacer(1, 54 * mm),
        Paragraph("Kendr Optimizer", STYLES["cover_title"]),
        Paragraph(
            "Verification-Gated Typed Token Reduction<br/>for Provider-Neutral LLM Contexts",
            STYLES["cover_subtitle"],
        ),
        HRFlowable(width="48%", thickness=2, color=CYAN, hAlign="LEFT", spaceAfter=10 * mm),
        metrics,
        Spacer(1, 12 * mm),
        Paragraph(
            "A deterministic, fail-open framework for structure-aware compaction, "
            "byte-exact reconstruction, cache protection, and auditable reduction evidence.",
            STYLES["cover_meta"],
        ),
        Spacer(1, 28 * mm),
        Paragraph(
            "Technical whitepaper v0.1<br/>August 2026<br/>Apache-2.0 project",
            STYLES["cover_meta"],
        ),
        Spacer(1, 8 * mm),
        Paragraph(
            f"Authoritative Markdown SHA-256: {source_sha256}",
            ParagraphStyle(
                "CoverDigest",
                parent=STYLES["cover_meta"],
                fontName=FONT_MONO,
                fontSize=6.7,
                textColor=LAVENDER,
            ),
        ),
        NextPageTemplate("Body"),
        PageBreak(),
        Paragraph("Contents", STYLES["toc_title"]),
        Paragraph(
            "The Markdown source is authoritative. Figures are vector renderings generated "
            "from explicit source markers.",
            STYLES["body"],
        ),
        Spacer(1, 4 * mm),
        toc,
        PageBreak(),
    ]


def build(source_path: Path, output_path: Path) -> None:
    for asset in (BRAND_MARK, BRAND_ICON):
        if not asset.is_file():
            raise FileNotFoundError(f"required publication asset is missing: {asset}")
    source = source_path.read_text(encoding="utf-8")
    source_sha256 = hashlib.sha256(source.encode("utf-8")).hexdigest()
    output_path.parent.mkdir(parents=True, exist_ok=True)
    story = cover_story(source_sha256) + parse_markdown(source)
    document = WhitepaperDocTemplate(
        str(output_path),
        title=(
            "Kendr Optimizer: Verification-Gated Typed Token Reduction for "
            "Provider-Neutral LLM Contexts"
        ),
        author="Kendr Optimizer contributors",
        creator="scripts/build_whitepaper.py",
        subject=(
            "Provider-neutral LLM context optimization technical whitepaper; "
            f"source SHA-256 {source_sha256}; ReportLab {REPORTLAB_VERSION}; "
            f"fonts {FONT_SOURCE}"
        ),
        keywords=(
            "Kendr Optimizer, token reduction, context optimization, LLM, verification, "
            "prompt compression, tool output"
        ),
        leftMargin=MARGIN_X,
        rightMargin=MARGIN_X,
        topMargin=MARGIN_TOP,
        bottomMargin=MARGIN_BOTTOM,
    )
    document.multiBuild(story, canvasmaker=DeterministicCanvas)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    build(args.source.resolve(), args.output.resolve())
    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
