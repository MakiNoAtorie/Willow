# Willow

A desktop app for planning stories as diagrams of rounded shapes and straight lines.

## Language

### Document

**Document**:
One file, holding one canvas.
_Avoid_: Project, board

**Canvas**:
The infinite, pannable and zoomable surface a document is drawn on.
_Avoid_: Board, workspace

**Recovery file**:
The autosaved copy of a document with unsaved changes, kept only for crash recovery; never a save.
_Avoid_: Backup, autosave file

### Shapes and chains

**Shape**:
A rounded rectangle holding rich text; the only content a canvas has.
_Avoid_: Node, box, card, block

**Link**:
A connection between two shapes. It has no direction and stores no sides.
_Avoid_: Edge, arrow, relation

**Connector**:
The straight line drawn for a link, from the side of each shape that faces the other.
_Avoid_: Line, wire

**Chain**:
The set of shapes reachable from each other through links; always a tree. An unlinked shape is a chain of one. A chain owns one color.
_Avoid_: Group, cluster, graph

**Side**:
Top, bottom, left or right of a shape, as faced by the other end of a link at draw time.
_Avoid_: Port, anchor

**Neutral**:
The light gray that new unlinked chains and new zones start with; one of the seven palette colors.
_Avoid_: Default color, gray, none

**Story order**:
The order of the saved text: blocks by topmost shape, each chain walked depth-first from its topmost shape (ADR 0002).
_Avoid_: File order, creation order

**Creation gap**:
The theme's edge-to-edge distance between a shape and a new or auto-placed shape linked to it.
_Avoid_: Spacing, margin

**Minimum gap**:
The theme's edge-to-edge distance a push keeps between a shape and another chain.
_Avoid_: Margin, padding

**Midpoint**:
One of the four circles at a hovered shape's edge midpoints; click to create a linked shape, drag to link.
_Avoid_: Handle, port, anchor

**Insert-and-push**:
Creating a shape from a midpoint where a shape is already linked on that side: the new shape goes between them and that shape's subtree moves along.
_Avoid_: Splice

**Push**:
A saved move the app makes to keep a gap when a shape grows or is created; it moves a linked subtree or a whole other chain as one block.
_Avoid_: Nudge, shove, reflow

**Hub**:
The neighbor of a deleted shape that its opposite-side neighbors relink to.

**Directional picker**:
The overlay Tab shows over a shape, from which an arrow key creates a linked shape on that side.
_Avoid_: Directional indicator

**Keyboard pan**:
Panning the view with the arrow keys while nothing is selected.
_Avoid_: Scroll

### Zones

**Zone**:
A titled, colored container whose member shapes and nested zones are listed explicitly, not found by geometry.
_Avoid_: Group, frame, section, container

**Member**:
A shape or nested zone that belongs to a zone.

### Look

**Theme**:
The app's whole look (font, palette values, shape and connector styles); Paper, Desk or Poster. An app preference, never part of a document.
_Avoid_: Skin, style

### Writer's reading

These are how the writer reads the canvas; the app has no concept of them.

**Plot trunk**:
A vertical chain describing the evolving situation of the story.
_Avoid_: Spine, main line

**Branch**:
A separate chain beside the trunk for what a character does.
_Avoid_: Subplot, side chain
