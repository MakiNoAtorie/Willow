// Willow theme tokens. Reference values for the visual-language document; mockups only, never shipped.
// Color order is fixed: neutral, rose, peach, butter, sage, sky, lilac (names are what the file format writes).
const COLOR_NAMES = ['neutral', 'rose', 'peach', 'butter', 'sage', 'sky', 'lilac'];

const THEMES = {
  paper: {
    name: 'Paper',
    font: "'Literata', Georgia, serif", fontSize: 15, lineHeight: 1.5, weight: 400, boldWeight: 700,
    uiFont: "-apple-system, 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif",
    codeFont: "'JetBrains Mono', ui-monospace, Menlo, monospace",
    ink: '#3b342c', inkSoft: 'rgba(59,52,44,.55)', accent: '#8b7760',
    canvas: '#f6f3ec', canvasTexture: 'radial-gradient(circle, rgba(70,50,20,.10) 1px, transparent 1.2px)', textureSize: 24,   // dot grid, hidden below zoom.fadeEnd

    shape: { width: 210, radius: 14, padX: 16, padY: 13,
             border: 'none',
             shadow: '0 1px 2px rgba(70,50,20,.07), 0 4px 14px rgba(70,50,20,.08)' },
    pastel: ['#e9e5dd', '#f3d8d3', '#f6dfc8', '#f2e8c0', '#d9e5cf', '#d3e0ea', '#e2d8ea'],
    picker: ['#a39b8f', '#d9776c', '#e3955a', '#d2ad3a', '#7ea468', '#5d8fbf', '#9479b8'],

    connector: { color: '#cbbfae', width: 2, hoverColor: '#8b7760', hoverWidth: 3, hitTolerance: 8 },
    selection: { ring: 2, gap: 2, color: '#8b7760',
                 shadow: '0 1px 2px rgba(70,50,20,.07), 0 6px 18px rgba(70,50,20,.12)' },
    midpoint: { size: 9, fill: '#ffffff', stroke: '#8b7760', strokeWidth: 1.5 },
    caret: { color: '#3b342c', width: 1.5 },
    textSelection: 'rgba(139,119,96,.22)',

    zone: { radius: 22, fillMix: 0.45, nestedFillMix: 0.7, borderMix: 0.35, titleSize: 13, titleWeight: 600, titleMinScreen: 12 },
    marquee: { fill: 'rgba(139,119,96,.08)', border: 'rgba(139,119,96,.65)', radius: 3 },
    picker2: { keycap: '#fffdf8', ghost: 'rgba(139,119,96,.55)' },   // directional picker
    zoomPill: { bg: 'rgba(59,52,44,.84)', ink: '#fffdf8' },

    menu: { bg: '#fffdf8', ink: '#3b342c', radius: 10, itemRadius: 6, size: 13, hover: '#f1ebe0',
            shadow: '0 0 0 1px rgba(70,50,20,.08), 0 10px 30px rgba(70,50,20,.18)', swatch: 18 },

    code: { bg: '#fbf9f4', border: 'rgba(70,50,20,.10)', size: 12.5, lineHeight: 1.65, gutter: '#c9bfb0',
            keyword: '#8b7760', id: '#3b342c', string: '#5f7d4c', dir: '#8a6fae', layout: '#a39b8f', invalid: '#d9776c' },

    chrome: { title: 'rgba(59,52,44,.55)', winButtonHover: 'rgba(70,50,20,.07)' },

    // Zoom: text fully visible at >= fadeStart, gone at <= fadeEnd (15 px * 0.5 = 7.5 px on screen, above the 7 px floor).
    zoom: { min: 0.1, max: 4, fadeStart: 0.75, fadeEnd: 0.5, blockMix: 0.3 },
    motion: { create: '160ms cubic-bezier(.2,.8,.2,1)', push: '220ms cubic-bezier(.2,.8,.2,1)',
              color: '200ms ease-out', select: '120ms ease-out', zoomStep: '120ms cubic-bezier(.2,.8,.2,1)',
              zoomPillHold: 700, zoomPillFade: '400ms ease-in' },
  },
};
