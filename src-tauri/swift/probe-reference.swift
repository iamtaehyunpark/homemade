import Foundation
import FoundationModels

@main struct Probe {
  static func main() async {
    let model = SystemLanguageModel.default
    switch model.availability {
    case .available: print("availability: available")
    case .unavailable(let r): print("availability: unavailable \(r)"); exit(2)
    }
    let session = LanguageModelSession(instructions: "You are a terse assistant. Answer with JSON only.")
    let t0 = Date()
    do {
      let r = try await session.respond(to: "Return a JSON object {\"name\": string, \"exec\": string} describing a Python CLI called transcribe.py that takes --input and --model. Nothing else.")
      print("latency_ms:", Int(Date().timeIntervalSince(t0)*1000))
      print(r.content)
    } catch { print("error:", error); exit(3) }
  }
}
