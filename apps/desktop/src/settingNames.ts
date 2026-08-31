/** Settings are named by their JSON key, which is the only name that exists
 *  for every one of them — a hand-kept table of prettier labels would be
 *  wrong for whichever setting was added last. Mirrors the native app's
 *  `ExternalSettingsChange.humanised`. */
export function humanisedSettingName(key: string): string {
  const words: string[] = [];
  let current = "";
  for (const character of key) {
    if (character >= "A" && character <= "Z" && current) {
      words.push(current);
      current = character.toLowerCase();
    } else {
      current += character;
    }
  }
  if (current) words.push(current);
  if (!words.length) return key;
  return [words[0].charAt(0).toUpperCase() + words[0].slice(1), ...words.slice(1)].join(" ");
}
