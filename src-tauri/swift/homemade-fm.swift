// homemade-fm: JSON Lines bridge to Apple's on-device Foundation Model (macOS 26+).
// stdin:  {"id":"1","op":"status"} | {"id":"2","op":"complete","system":"..","prompt":"..","max_tokens":1024,"temperature":0.2,"json":true}
// stdout: one JSON response per request, strictly in order. stderr: free-form logs. Exits when stdin closes.
import Foundation
import FoundationModels

func emit(_ obj: [String: Any]) {
  guard let data = try? JSONSerialization.data(withJSONObject: obj, options: []),
        var s = String(data: data, encoding: .utf8) else { return }
  s += "\n"
  FileHandle.standardOutput.write(s.data(using: .utf8)!)
}

func log(_ s: String) { FileHandle.standardError.write(("[homemade-fm] " + s + "\n").data(using: .utf8)!) }

func statusPayload() -> [String: Any] {
  switch SystemLanguageModel.default.availability {
  case .available:
    return ["ok": true, "available": true]
  case .unavailable(let reason):
    let r: String
    switch reason {
    case .appleIntelligenceNotEnabled: r = "appleIntelligenceNotEnabled"
    case .deviceNotEligible: r = "deviceNotEligible"
    case .modelNotReady: r = "modelNotReady"
    @unknown default: r = String(describing: reason)
    }
    return ["ok": true, "available": false, "reason": r]
  }
}

/// Strip ``` fences and return the first balanced JSON object/array, else the trimmed text.
func extractJSON(_ text: String) -> String {
  var t = text.trimmingCharacters(in: .whitespacesAndNewlines)
  if t.hasPrefix("```") {
    if let nl = t.firstIndex(of: "\n") { t = String(t[t.index(after: nl)...]) }
    if let end = t.range(of: "```", options: .backwards) { t = String(t[..<end.lowerBound]) }
    t = t.trimmingCharacters(in: .whitespacesAndNewlines)
  }
  let chars = Array(t)
  guard let start = chars.firstIndex(where: { $0 == "{" || $0 == "[" }) else { return t }
  var stack: [Character] = []
  var inStr = false, esc = false
  for i in start..<chars.count {
    let c = chars[i]
    if inStr {
      if esc { esc = false } else if c == "\\" { esc = true } else if c == "\"" { inStr = false }
      continue
    }
    switch c {
    case "\"": inStr = true
    case "{", "[": stack.append(c)
    case "}", "]":
      guard let open = stack.popLast(), (open == "{" && c == "}") || (open == "[" && c == "]") else { return t }
      if stack.isEmpty { return String(chars[start...i]) }
    default: break
    }
  }
  return t
}

@main struct HomemadeFM {
  static func main() async {
    log("ready")
    while let line = readLine(strippingNewline: true) {
      let trimmed = line.trimmingCharacters(in: .whitespaces)
      if trimmed.isEmpty { continue }
      guard let data = trimmed.data(using: .utf8),
            let req = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
        emit(["id": NSNull(), "ok": false, "error": "invalid JSON request"]); continue
      }
      let id: Any = req["id"] ?? NSNull()
      let op = req["op"] as? String ?? ""
      switch op {
      case "status":
        var p = statusPayload(); p["id"] = id; emit(p)
      case "complete":
        let system = req["system"] as? String ?? ""
        let prompt = req["prompt"] as? String ?? ""
        let maxTokens = (req["max_tokens"] as? NSNumber)?.intValue ?? 1024
        let temperature = (req["temperature"] as? NSNumber)?.doubleValue ?? 0.2
        let wantJSON = req["json"] as? Bool ?? false
        if case .unavailable = SystemLanguageModel.default.availability {
          var p = statusPayload()
          emit(["id": id, "ok": false, "error": "model unavailable: \(p.removeValue(forKey: "reason") ?? "unknown")"]); continue
        }
        let session = LanguageModelSession(instructions: system)
        let opts = GenerationOptions(temperature: temperature, maximumResponseTokens: maxTokens)
        let t0 = Date()
        do {
          let r = try await session.respond(to: prompt, options: opts)
          log("complete id=\(id) \(Int(Date().timeIntervalSince(t0) * 1000))ms")
          let text = wantJSON ? extractJSON(r.content) : r.content
          emit(["id": id, "ok": true, "text": text])
        } catch {
          emit(["id": id, "ok": false, "error": String(describing: error)])
        }
      default:
        emit(["id": id, "ok": false, "error": "unknown op: \(op)"])
      }
    }
  }
}
