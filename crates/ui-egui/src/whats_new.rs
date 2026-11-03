//! What's new: the home screen's row of cards, one per feature a release added (newest first), as the reference
//! shows its own (s30-011: a dark card, a white title, a picture of the feature). Each card names the version
//! that brought the feature, and links to the changelog for every version's notes.
//!
//! Adding a release: put its features at the top, with a picture in `assets/home/` (our own renders or
//! screenshots, 340 × 438, see-through around the model) and a row in ATTRIBUTION.md if it's a new kind of file.

/// Every version's notes.
pub const NOTES_URL: &str = "https://github.com/jackharvest/shapecraft/blob/main/CHANGELOG.md";

pub struct Entry {
    pub version: &'static str,
    pub title: &'static str,
    /// What the detail card says.
    pub text: &'static str,
    /// The picture's name in `assets/home/` (without `.png`).
    pub art: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        version: "0.3.0",
        title: "Learn in 7 minutes",
        text: "Three short in-app tutorials (a phone stand, a pencil cup and a key tag) walk you through sketching, \
               pulling, cutting, rounding and finishing. Each step starts fresh, so there's nothing to undo. Find them \
               under Get Started and Tutorials.",
        art: "new-tutorials",
    },
    Entry {
        version: "0.3.0",
        title: "Grab the cube to turn the view",
        text: "Drag the view cube in the top-right corner to orbit freely; it stays where you let go. Clicking a face, \
               edge or corner still jumps to that view, and double-clicking goes back to the default.",
        art: "new-view-cube",
    },
    Entry {
        version: "0.2.0",
        title: "Parameter Cards on canvas",
        text: "Fillet and Split Body show a card with every setting of the step, both while the tool runs and when you \
               edit it from History. Drag the card anywhere; its ⋯ menu decides when it opens.",
        art: "new-cards",
    },
    Entry {
        version: "0.2.0",
        title: "Linked and unlinked copies",
        text: "Copies made with Move or Rotate Around Axis can stay linked to the original, or become their own \
               bodies with the Link badge turned off.",
        art: "new-copies",
    },
    Entry {
        version: "0.2.0",
        title: "Eleven languages",
        text: "Preferences ▸ Language switches the toolbar, sidebars and cards to one of eleven languages.",
        art: "new-languages",
    },
    Entry {
        version: "0.1.0",
        title: "Your Shapr3D projects open here",
        text: ".shapr files open with their bodies, sketches and steps, and projects you save from ShapeCraft keep \
               their editable history.",
        art: "new-shapr",
    },
];
