import AppKit
import Foundation

private let overlayWidth: CGFloat = 380
private let headerHeight: CGFloat = 44
private let rowHeight: CGFloat = 54
private let emptyHeight: CGFloat = 50

final class OverlayView: NSView {
    var rows: [ControllerRow] = [] {
        didSet { needsDisplay = true }
    }
    var onRightClick: (() -> Void)?

    override var isFlipped: Bool { true }

    override func rightMouseUp(with event: NSEvent) {
        onRightClick?()
    }

    override func draw(_ dirtyRect: NSRect) {
        let background = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 12, yRadius: 12)
        NSColor(calibratedRed: 0.07, green: 0.09, blue: 0.12, alpha: 0.9).setFill()
        background.fill()
        NSColor(calibratedWhite: 1, alpha: 0.16).setStroke()
        background.lineWidth = 1
        background.stroke()

        drawText("Gamepad Battery", in: NSRect(x: 18, y: 13, width: 340, height: 26),
                 size: 17, color: .white, weight: .semibold)

        if rows.isEmpty {
            drawText("No gamepad connected", in: NSRect(x: 18, y: 52, width: 340, height: 24),
                     size: 14, color: NSColor(calibratedWhite: 0.75, alpha: 1))
        }

        for (index, row) in rows.enumerated() {
            let y = headerHeight + CGFloat(index) * rowHeight
            drawText(row.name, in: NSRect(x: 18, y: y, width: 340, height: 22),
                     size: 14, color: .white, weight: .medium)
            drawText(row.status, in: NSRect(x: 18, y: y + 23, width: row.percent == nil ? 340 : 265, height: 20),
                     size: 13, color: NSColor(calibratedRed: 0.68, green: 0.83, blue: 0.87, alpha: 1))
            if let percent = row.percent {
                drawBattery(percent: percent, at: NSPoint(x: 293, y: y + 27))
            }
        }
    }

    private func drawText(_ text: String, in rect: NSRect, size: CGFloat, color: NSColor,
                          weight: NSFont.Weight = .regular) {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        (text as NSString).draw(in: rect, withAttributes: [
            .font: NSFont.systemFont(ofSize: size, weight: weight),
            .foregroundColor: color,
            .paragraphStyle: style,
        ])
    }

    private func drawBattery(percent: Int, at point: NSPoint) {
        let frame = NSRect(x: point.x, y: point.y, width: 64, height: 16)
        let border = NSBezierPath(roundedRect: frame, xRadius: 4, yRadius: 4)
        NSColor(calibratedWhite: 1, alpha: 0.4).setStroke()
        border.stroke()
        let fillWidth = CGFloat(percent) / 100 * 58
        if fillWidth > 0 {
            let fill = NSBezierPath(roundedRect: NSRect(x: point.x + 3, y: point.y + 3,
                                                       width: fillWidth, height: 10), xRadius: 2, yRadius: 2)
            let color = percent <= 25 ? NSColor.systemRed : percent <= 60 ? NSColor.systemOrange :
                NSColor(calibratedRed: 0.40, green: 0.80, blue: 1, alpha: 1)
            color.setFill()
            fill.fill()
        }
        NSBezierPath(roundedRect: NSRect(x: point.x + 67, y: point.y + 5, width: 3, height: 6),
                     xRadius: 1, yRadius: 1).fill()
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var scanner: ControllerScanner!
    private var panel: NSPanel!
    private var overlay: OverlayView!
    private var statusItem: NSStatusItem!
    private var toggleItem: NSMenuItem!
    private var timer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        overlay = OverlayView(frame: NSRect(x: 0, y: 0, width: overlayWidth, height: 94))
        panel = NSPanel(contentRect: overlay.bounds, styleMask: [.borderless, .nonactivatingPanel],
                        backing: .buffered, defer: false)
        panel.title = "Gamepad Battery Tracker"
        panel.contentView = overlay
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.isMovableByWindowBackground = true
        overlay.onRightClick = { [weak self] in self?.hideOverlay() }

        if let screen = NSScreen.main {
            let frame = screen.visibleFrame
            panel.setFrameOrigin(NSPoint(x: frame.maxX - overlayWidth - 24, y: frame.maxY - 118))
        }
        panel.orderFront(nil)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            if let image = NSImage(systemSymbolName: "gamecontroller", accessibilityDescription: "Gamepad Battery Tracker") {
                button.image = image
            } else {
                button.title = "🎮"
            }
            button.toolTip = "Gamepad Battery Tracker"
        }
        let menu = NSMenu()
        toggleItem = NSMenuItem(title: "Hide Overlay", action: #selector(toggleOverlay), keyEquivalent: "")
        toggleItem.target = self
        menu.addItem(toggleItem)
        menu.addItem(.separator())
        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
        statusItem.menu = menu

        scanner = ControllerScanner()
        scanner.onChange = { [weak self] rows in self?.update(rows) }
        scanner.refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            self?.scanner.refresh()
        }
    }

    private func update(_ rows: [ControllerRow]) {
        let height = headerHeight + (rows.isEmpty ? emptyHeight : CGFloat(rows.count) * rowHeight) + 8
        let old = panel.frame
        let new = NSRect(x: old.minX, y: old.maxY - height, width: overlayWidth, height: height)
        panel.setFrame(new, display: true)
        overlay.rows = rows
    }

    private func hideOverlay() {
        panel.orderOut(nil)
        toggleItem.title = "Show Overlay"
    }

    @objc private func toggleOverlay() {
        if panel.isVisible {
            hideOverlay()
        } else {
            panel.orderFrontRegardless()
            toggleItem.title = "Hide Overlay"
        }
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

@main
struct EntryPoint {
    static func main() {
        if CommandLine.arguments.contains("--once") {
            let scanner = ControllerScanner()
            scanner.refresh()
            RunLoop.main.run(until: Date().addingTimeInterval(3))
            let rows = scanner.snapshot()
            if rows.isEmpty {
                print("No gamepad connected")
            } else {
                for row in rows { print("\(row.name): \(row.status)") }
            }
        } else {
            let app = NSApplication.shared
            let delegate = AppDelegate()
            app.delegate = delegate
            app.run()
        }
    }
}
