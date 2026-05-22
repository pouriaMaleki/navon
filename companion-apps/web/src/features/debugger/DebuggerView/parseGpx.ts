export function parseGpx(content: string): { latitude: number; longitude: number }[] {
  const points: { latitude: number; longitude: number }[] = [];
  const trkptRegex = /<trkpt[^>]*lat="([^"]+)"[^>]*lon="([^"]+)"/g;
  let match: RegExpExecArray | null;
  while ((match = trkptRegex.exec(content)) !== null) {
    points.push({
      latitude: Number.parseFloat(match[1]),
      longitude: Number.parseFloat(match[2]),
    });
  }
  // Also try rtept
  if (points.length === 0) {
    const rteptRegex = /<rtept[^>]*lat="([^"]+)"[^>]*lon="([^"]+)"/g;
    while ((match = rteptRegex.exec(content)) !== null) {
      points.push({
        latitude: Number.parseFloat(match[1]),
        longitude: Number.parseFloat(match[2]),
      });
    }
  }
  return points;
}
