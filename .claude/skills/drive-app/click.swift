// Posts mouse events at global screen points: click <left|right|double|move|down|drag|up> X Y
import CoreGraphics
import Foundation

let a = CommandLine.arguments
let p = CGPoint(x: Double(a[2])!, y: Double(a[3])!)
func post(_ t: CGEventType, _ b: CGMouseButton = .left, clicks: Int64 = 1) {
    let e = CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: b)!
    e.setIntegerValueField(.mouseEventClickState, value: clicks)
    e.post(tap: .cghidEventTap)
}
switch a[1] {
case "move": post(.mouseMoved)
case "down": post(.mouseMoved); usleep(100_000); post(.leftMouseDown)
// Ten steps from wherever the cursor is, so the app sees a moving drag.
case "drag":
    let from = CGEvent(source: nil)!.location
    for i in 1...10 {
        let t = Double(i) / 10
        let q = CGPoint(x: from.x + (p.x - from.x) * t, y: from.y + (p.y - from.y) * t)
        CGEvent(mouseEventSource: nil, mouseType: .leftMouseDragged, mouseCursorPosition: q,
                mouseButton: .left)!.post(tap: .cghidEventTap)
        usleep(16_000)
    }
case "up": post(.leftMouseUp)
case "double":
    post(.mouseMoved); usleep(100_000)
    post(.leftMouseDown); post(.leftMouseUp)
    usleep(50_000)
    post(.leftMouseDown, clicks: 2); post(.leftMouseUp, clicks: 2)
default:
    let right = a[1] == "right"
    post(.mouseMoved)
    usleep(100_000)
    post(right ? .rightMouseDown : .leftMouseDown, right ? .right : .left)
    usleep(50_000)
    post(right ? .rightMouseUp : .leftMouseUp, right ? .right : .left)
}
