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

**Directional picker**:
The overlay Tab shows over a shape, from which an arrow key creates a linked shape on that side.
_Avoid_: Directional indicator

### Zones

**Zone**:
A titled, colored container whose member shapes and nested zones are listed explicitly, not found by geometry.
_Avoid_: Group, frame, section, container

**Member**:
A shape or nested zone that belongs to a zone.

### Writer's reading

These are how the writer reads the canvas; the app has no concept of them.

**Plot trunk**:
A vertical chain describing the evolving situation of the story.
_Avoid_: Spine, main line

**Branch**:
A separate chain beside the trunk for what a character does.
_Avoid_: Subplot, side chain
