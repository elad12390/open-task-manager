// Render Finder icons to 32x32 PNGs: app bundles, executables, folders, and .icns files.
// Usage: osascript -l JavaScript app_icon.js <key> <png-path> [<key> <png-path>...]
// Prints "<key>\t<png-path>" for every icon written.
ObjC.import('AppKit');

function iconFor(path) {
  if (path.endsWith('.icns')) {
    const image = $.NSImage.alloc.initWithContentsOfFile(path);
    if (!image.isNil()) return image;
  }
  return $.NSWorkspace.sharedWorkspace.iconForFile(path);
}

function run(argv) {
  const lines = [];
  for (let i = 0; i + 1 < argv.length; i += 2) {
    const key = argv[i];
    const out = argv[i + 1];
    const image = iconFor(key);
    const size = 32;
    const rep = $.NSBitmapImageRep.alloc.initWithBitmapDataPlanesPixelsWidePixelsHighBitsPerSampleSamplesPerPixelHasAlphaIsPlanarColorSpaceNameBytesPerRowBitsPerPixel(
      null, size, size, 8, 4, true, false, $.NSCalibratedRGBColorSpace, 0, 0);
    $.NSGraphicsContext.saveGraphicsState;
    $.NSGraphicsContext.setCurrentContext($.NSGraphicsContext.graphicsContextWithBitmapImageRep(rep));
    image.drawInRectFromRectOperationFraction($.NSMakeRect(0, 0, size, size), $.NSZeroRect, $.NSCompositingOperationSourceOver, 1.0);
    $.NSGraphicsContext.restoreGraphicsState;
    const png = rep.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $());
    if (png.writeToFileAtomically(out, true)) lines.push(key + '\t' + out);
  }
  return lines.join('\n');
}
