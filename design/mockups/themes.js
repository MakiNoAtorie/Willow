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
            shadow: '0 0 0 1px rgba(70,50,20,.08), 0 10px 30px rgba(70,50,20,.18)', swatch: 18, danger: '#b5463b' },

    code: { bg: '#fbf9f4', border: 'rgba(70,50,20,.10)', size: 12.5, lineHeight: 1.65, gutter: '#c9bfb0',
            keyword: '#8b7760', id: '#3b342c', string: '#5f7d4c', dir: '#8a6fae', layout: '#a39b8f', invalid: '#d9776c' },

    chrome: { title: 'rgba(59,52,44,.55)', winButtonHover: 'rgba(70,50,20,.07)' },

    // Zoom: text fully visible at >= fadeStart, gone at <= fadeEnd (15 px * 0.5 = 7.5 px on screen, above the 7 px floor).
    zoom: { min: 0.1, max: 4, fadeStart: 0.75, fadeEnd: 0.5, blockMix: 0.3 },
    motion: { create: '160ms cubic-bezier(.2,.8,.2,1)', push: '220ms cubic-bezier(.2,.8,.2,1)',
              color: '200ms ease-out', select: '120ms ease-out', zoomStep: '120ms cubic-bezier(.2,.8,.2,1)',
              zoomPillHold: 700, zoomPillFade: '400ms ease-in' },
  },
  desk: {
    name: 'Desk',
    font: "'Courier Prime', 'Courier New', monospace", fontSize: 14, lineHeight: 1.5, weight: 400, boldWeight: 700,
    uiFont: "-apple-system, 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif",
    codeFont: "'Courier Prime', 'Courier New', monospace",
    ink: '#2b2621', inkSoft: 'rgba(43,38,33,.55)', accent: '#2b2621',
    canvas: '#e6dfd1', textureSize: null,   // linen weave, drawn at a fixed screen size
    canvasTexture: 'repeating-linear-gradient(0deg, rgba(120,100,70,.035) 0 1px, transparent 1px 3px), repeating-linear-gradient(90deg, rgba(120,100,70,.03) 0 1px, transparent 1px 4px)',

    shape: { width: 210, radius: 4, padX: 15, padY: 13, padTop: 12, tab: 6,   // tab: a strip of the picker color along the top
             border: 'none',
             shadow: '0 1px 1px rgba(70,50,20,.18), 0 4px 10px rgba(70,50,20,.14)' },
    pastel: ['#fbfaf6', '#fff3f1', '#fff6ec', '#fffbea', '#f5fbf1', '#f2f7fc', '#f8f4fc'],
    picker: ['#b9b1a3', '#d8574c', '#e8873a', '#e2b52c', '#5c9d4d', '#4a84c4', '#8a66c0'],

    connector: { color: '#9b8c74', width: 1.5, hoverColor: '#2b2621', hoverWidth: 2.5, hitTolerance: 8, cap: 'round' },
    selection: { boxShadow: '0 0 0 1.5px #2b2621, 0 16px 30px rgba(70,50,20,.28), 0 3px 6px rgba(70,50,20,.12)', transform: 'translateY(-4px)' },
    midpoint: { size: 10, fill: '#c79a3e', stroke: '#fff7e6', strokeWidth: 1.5 },
    caret: { color: '#2b2621', width: 1.5 },
    textSelection: 'rgba(199,154,62,.32)',

    zone: { radius: 6, fillFrom: 'picker', fillMix: 0.16, nestedFillMix: 0.3, border: 'none', titleSize: 12, titleWeight: 700, titleMinScreen: 12,
            titleCss: { textTransform: 'uppercase', letterSpacing: '.08em' }, shadow: 'inset 0 1px 2px rgba(70,50,20,.12)' },
    marquee: { fill: 'rgba(43,38,33,.05)', border: 'rgba(43,38,33,.6)', borderStyle: 'dashed', radius: 2 },
    picker2: { keycap: '#fffdf7', ghost: 'rgba(43,38,33,.45)', radius: 3, shadow: '0 1px 1px rgba(70,50,20,.25), 0 3px 6px rgba(70,50,20,.15)' },
    zoomPill: { bg: 'rgba(43,38,33,.88)', ink: '#fffdf7' },

    menu: { bg: '#fffdf7', ink: '#2b2621', radius: 4, itemRadius: 2, size: 13, hover: '#f3ead6', font: "'Courier Prime', monospace", weight: 400,
            shadow: '0 1px 1px rgba(70,50,20,.2), 0 14px 30px rgba(70,50,20,.25)', swatch: 18, swatchRadius: 3, danger: '#b8483e' },

    code: { bg: '#fbf8f1', border: 'rgba(70,50,20,.15)', size: 13, lineHeight: 1.6, gutter: '#c4b8a2',
            keyword: '#8a6a2c', id: '#2b2621', string: '#5c7d46', dir: '#7a5aa8', layout: '#a89d8a', invalid: '#d8574c' },

    chrome: { title: 'rgba(43,38,33,.55)', winButtonHover: 'rgba(70,50,20,.08)' },

    // Courier Prime 14 px: 7.3 px on screen at 52%.
    zoom: { min: 0.1, max: 4, fadeStart: 0.75, fadeEnd: 0.52, blockMix: 0.6 },   // near-white cards need more depth to read as blocks
    motion: { create: '160ms cubic-bezier(.2,.8,.2,1)', push: '220ms cubic-bezier(.2,.8,.2,1)',
              color: '200ms ease-out', select: '160ms cubic-bezier(.2,.8,.2,1)', zoomStep: '120ms cubic-bezier(.2,.8,.2,1)',
              zoomPillHold: 700, zoomPillFade: '400ms ease-in' },
  },
  poster: {
    name: 'Poster',
    font: "'Space Grotesk', system-ui, sans-serif", fontSize: 14.5, lineHeight: 1.45, weight: 500, boldWeight: 700,
    uiFont: "-apple-system, 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif",
    codeFont: "'JetBrains Mono', ui-monospace, Menlo, monospace",
    ink: '#111111', inkSoft: 'rgba(17,17,17,.6)', accent: '#111111',
    canvas: '#fbf6ea', canvasTexture: 'none', textureSize: null,

    shape: { width: 210, radius: 10, padX: 14, padY: 12,
             border: '2.5px solid #111111',
             shadow: '4px 4px 0 #111111' },
    pastel: ['#e9e7e1', '#ffb8b0', '#ffc994', '#ffe57a', '#b6eda9', '#a9d6ff', '#d2bbff'],
    picker: ['#8f8b82', '#ff4f3f', '#ff8a1f', '#ffc800', '#2fbf4a', '#1f8bff', '#8b4dff'],

    connector: { color: '#111111', width: 2.5, hoverColor: '#ff4d1a', hoverWidth: 4.5, hitTolerance: 8, cap: 'square' },
    selection: { boxShadow: '8px 8px 0 #111111', transform: 'translate(-3px, -3px)' },
    midpoint: { size: 11, fill: '#ffffff', stroke: '#111111', strokeWidth: 2.5 },
    caret: { color: '#111111', width: 2 },
    textSelection: '#ffe066',

    zone: { radius: 16, fillMix: 0.5, nestedFillMix: 0.85, border: '2.5px solid #111111', titleSize: 12, titleWeight: 700, titleMinScreen: 12,
            titleCss: { textTransform: 'uppercase', letterSpacing: '.06em', background: '#111111', color: '#ffffff', padding: '3px 9px', borderRadius: '999px' } },
    marquee: { fill: 'rgba(255,224,102,.25)', border: '#111111', borderStyle: 'dashed', borderWidth: 2, radius: 4 },
    picker2: { keycap: '#ffffff', ghost: '#111111', radius: 6, border: '2.5px solid #111111', shadow: '3px 3px 0 #111111' },
    zoomPill: { bg: '#111111', ink: '#ffe57a' },

    menu: { bg: '#ffffff', ink: '#111111', radius: 8, itemRadius: 4, size: 13, hover: '#ffe57a', font: "'Space Grotesk', system-ui", weight: 600,
            shadow: '0 0 0 2.5px #111111, 5px 5px 0 2.5px #111111', swatch: 22, swatchRadius: 6, swatchRing: '#111111', danger: '#e03131' },

    code: { bg: '#fffaf0', border: '#111111', borderWidth: 2.5, size: 12.5, lineHeight: 1.65, gutter: '#b5ae9f',
            keyword: '#d9480f', id: '#111111', string: '#2b8a3e', dir: '#6741d9', layout: '#8f8b82', invalid: '#ff4f3f' },

    chrome: { title: 'rgba(17,17,17,.6)', winButtonHover: 'rgba(17,17,17,.08)' },

    // Space Grotesk 14.5 px: 7.25 px on screen at 50%.
    zoom: { min: 0.1, max: 4, fadeStart: 0.75, fadeEnd: 0.5, blockMix: 0.2 },
    motion: { create: '160ms cubic-bezier(.3,1.4,.5,1)', push: '220ms cubic-bezier(.2,.8,.2,1)',
              color: '160ms steps(2, end)', select: '100ms cubic-bezier(.3,1.6,.5,1)', zoomStep: '120ms cubic-bezier(.2,.8,.2,1)',
              zoomPillHold: 700, zoomPillFade: '300ms ease-in' },
  },
};

// ---------- shared helpers (mockups.html, motion.html) ----------
const mix = (a, b, t) => t <= 0 ? a : `color-mix(in srgb, ${b} ${Math.round(t * 100)}%, ${a})`;

// CSS for one shape. deepen: 0..1 toward the low-zoom block color. shadowOn: false drops the resting shadow.
function shapeStyle(T, c, { selected = false, deepen = 0, shadowOn = true } = {}) {
  const S = T.shape, sel = T.selection;
  const ring = sel.boxShadow || `0 0 0 ${sel.gap}px ${T.canvas}, 0 0 0 ${sel.gap + sel.ring}px ${sel.color}, ${sel.shadow}`;
  const rest = sel.boxShadow ? S.shadow : `0 0 0 0 ${T.canvas}, 0 0 0 0 ${sel.color}, ${S.shadow}`;   // same layer count so it can animate
  return {
    padding: `${S.padTop ?? S.padY}px ${S.padX}px ${S.padY}px`, borderRadius: S.radius + 'px', border: S.border,
    ...(S.tab ? { borderTop: `${S.tab}px solid ${T.picker[c]}` } : {}),
    backgroundColor: mix(T.pastel[c], T.picker[c], T.zoom.blockMix * deepen),
    boxShadow: selected ? ring : shadowOn ? rest : 'none',
    transform: selected && sel.transform ? sel.transform : 'none',
    color: T.ink, font: `${T.weight} ${T.fontSize}px/${T.lineHeight} ${T.font}`,
  };
}
