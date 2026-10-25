// Posts one mouse click at global screen points: click <left|right> X Y
import CoreGraphics
import Foundation

let a = CommandLine.arguments
let right = a[1] == "right"
let p = CGPoint(x: Double(a[2])!, y: Double(a[3])!)
let (down, up, button): (CGEventType, CGEventType, CGMouseButton) =
    right ? (.rightMouseDown, .rightMouseUp, .right) : (.leftMouseDown, .leftMouseUp, .left)
func post(_ t: CGEventType, _ b: CGMouseButton) {
    CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: b)!
        .post(tap: .cghidEventTap)
}
post(.mouseMoved, .left)
usleep(100_000)
post(down, button)
usleep(50_000)
post(up, button)
