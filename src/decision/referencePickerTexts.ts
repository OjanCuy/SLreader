// UI texts of the ReferencePicker (Brief Sections 7 and 8, approved in T-26).

export const PICKER_LABEL = "Link to Reference No";
export const PICKER_PLACEHOLDER = "Search references";
export const BEST_MATCH = "Best match";
export const NO_MATCHES = "No matching references.";
export const NO_REFERENCES = "Import references to link this paper.";

export const referenceName = (no: number, title: string) => `${no} - ${title}`;
export const takenBy = (paper: string) => `Linked to another paper: ${paper}`;
export const takenMessage = (no: number, paper: string) => `Reference No ${no} is already linked to ${paper}.`;
export const replaceMetadataMessage = (no: number) =>
  `The metadata of this paper will be replaced with the data of Reference No ${no}.`;
