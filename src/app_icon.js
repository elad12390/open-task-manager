// Render each app bundle's Finder icon to a 32x32 PNG.
// Usage: osascript -l JavaScript app_icon.js <out-dir> <bundle-path>...
// Prints "<bundle-path>\t<png-path>" for every icon written.
ObjC.import('AppKit');

function run(argv) {
  const outDir = argv[0];
  const lines = [];
  for (const bundle of argv.slice(1)) {
    const image = $.NSWorkspace.sharedWorkspace.iconForFile(bundle);
    const size = 32;
    const rep = $.NSBitmapImageRep.alloc.initWithBitmapDataPlanesPixelsWidePixelsHighBitsPerSampleSamplesPerPixelHasAlphaIsPlanarColorSpaceNameBytesPerRowBitsPerPixel(
      null, size, size, 8, 4, true, false, $.NSCalibratedRGBColorSpace, 0, 0);
    $.NSGraphicsContext.saveGraphicsState;
    $.NSGraphicsContext.setCurrentContext($.NSGraphicsContext.graphicsContextWithBitmapImageRep(rep));
    image.drawInRectFromRectOperationFraction($.NSMakeRect(0, 0, size, size), $.NSZeroRect, $.NSCompositingOperationSourceOver, 1.0);
    $.NSGraphicsContext.restoreGraphicsState;
    const png = rep.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $());
    const name = bundle.replace(/[^A-Za-z0-9]+/g, '_') + '.png';
    const out = outDir + '/' + name;
    if (png.writeToFileAtomically(out, true)) lines.push(bundle + '\t' + out);
  }
  return lines.join('\n');
}
