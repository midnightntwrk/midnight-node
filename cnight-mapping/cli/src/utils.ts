
// Helper function to convert to JSON for logging
export const toJson = (obj: object): string => {
  return JSON.stringify(obj, (_key, value) => (typeof value === 'bigint' ? value.toString() + 'n' : value), 2);
};

