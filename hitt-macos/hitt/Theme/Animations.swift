import SwiftUI

extension HittTheme {
    enum Animations {
        static let quick = Animation.spring(duration: 0.2, bounce: 0.1)
        static let smooth = Animation.spring(duration: 0.35, bounce: 0.15)
        static let spring = Animation.spring(duration: 0.45, bounce: 0.25)
        static let ease = Animation.easeInOut(duration: 0.2)
    }
}
