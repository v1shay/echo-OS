import AppKit
import Foundation
import Vision

struct OCRBox: Codable {
    let text: String
    let confidence: Float
    let x: Double
    let y: Double
    let width: Double
    let height: Double
}

struct OCRResult: Codable {
    let width: Double
    let height: Double
    let fullText: String
    let boxes: [OCRBox]
}

enum OCRToolError: Error {
    case invalidArguments
    case imageLoadFailed
}

func cgImage(from path: String) throws -> CGImage {
    guard
        let image = NSImage(contentsOfFile: path),
        let tiff = image.tiffRepresentation,
        let bitmap = NSBitmapImageRep(data: tiff),
        let cgImage = bitmap.cgImage
    else {
        throw OCRToolError.imageLoadFailed
    }
    return cgImage
}

func recognize(path: String) throws -> OCRResult {
    let image = try cgImage(from: path)
    let imageWidth = Double(image.width)
    let imageHeight = Double(image.height)

    var observed: [OCRBox] = []
    let request = VNRecognizeTextRequest { request, _ in
        guard let observations = request.results as? [VNRecognizedTextObservation] else {
            return
        }
        for observation in observations {
            guard let candidate = observation.topCandidates(1).first else { continue }
            let rect = observation.boundingBox
            observed.append(
                OCRBox(
                    text: candidate.string,
                    confidence: candidate.confidence,
                    x: rect.origin.x * imageWidth,
                    y: (1.0 - rect.origin.y - rect.height) * imageHeight,
                    width: rect.width * imageWidth,
                    height: rect.height * imageHeight
                )
            )
        }
    }
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: image)
    try handler.perform([request])

    let fullText = observed.map(\.text).joined(separator: "\n")
    return OCRResult(width: imageWidth, height: imageHeight, fullText: fullText, boxes: observed)
}

guard CommandLine.arguments.count == 2 else {
    fputs("usage: jarvis_ocr <image-path>\n", stderr)
    exit(2)
}

do {
    let result = try recognize(path: CommandLine.arguments[1])
    let data = try JSONEncoder().encode(result)
    FileHandle.standardOutput.write(data)
} catch {
    fputs("OCR failure: \(error)\n", stderr)
    exit(1)
}
