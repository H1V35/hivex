// Existing snapshots and work keys use JavaScript's UTF-16 string order.
export const compareSerializedStrings = function compareSerializedStrings(
  left: string,
  right: string
) {
  const leftUnits = Buffer.from(left, "utf-16le").swap16();
  const rightUnits = Buffer.from(right, "utf-16le").swap16();
  return leftUnits.compare(rightUnits);
};
