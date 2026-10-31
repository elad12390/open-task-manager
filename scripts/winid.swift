// Print the window number of the largest on-screen window owned by a process.
// Usage: winid <pid>
import CoreGraphics
import Foundation

guard CommandLine.arguments.count > 1, let pid = Int32(CommandLine.arguments[1]) else { exit(2) }
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let width = { (w: [String: Any]) -> Double in (w[kCGWindowBounds as String] as? [String: Double])?["Width"] ?? 0 }
let best = windows
    .filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && width($0) > 200 }
    .max { width($0) < width($1) }
if let number = best?[kCGWindowNumber as String] as? Int { print(number) } else { exit(1) }
