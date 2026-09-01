// In-place DOM patching for render() (D9 native-text audit, finding 3).
//
// Rebuilding root.innerHTML wholesale replaced every node on each 900 ms
// snapshot tick and on each transcript-search keystroke. WebKit binds a text
// field's undo stack to the element itself, so a recreated editor kept its
// value and focus but lost its cmd-Z history — and the transcript-search
// input sat inside a re-created, collapsed <details>, so the first keystroke
// dropped focus outright. Patching mutates the existing tree instead: a node
// whose rendered content did not change is never detached, which keeps focus,
// selection, scroll position, and the undo stack alive across renders.
//
// Matching rules, deliberately minimal:
// - An element carrying data-field is keyed by data-field + data-meeting-id,
//   so an editor is reused only for the same field of the same meeting.
// - An element with an id is keyed by that id.
// - Everything else matches positionally by tag name.
// A keyed editor that stays in position is morphed without ever being
// detached; that invariant is what preserves the undo stack.

function nodeKey(node) {
  if (node.nodeType !== Node.ELEMENT_NODE) return null;
  const field = node.getAttribute("data-field");
  if (field) return `field:${field}:${node.getAttribute("data-meeting-id") || ""}`;
  const id = node.getAttribute("id");
  return id ? `id:${id}` : null;
}

function compatible(oldNode, newNode) {
  if (oldNode.nodeType !== newNode.nodeType) return false;
  if (oldNode.nodeType !== Node.ELEMENT_NODE) return true;
  return oldNode.tagName === newNode.tagName && nodeKey(oldNode) === nodeKey(newNode);
}

function syncAttributes(oldNode, newNode) {
  for (const attribute of Array.from(newNode.attributes)) {
    if (oldNode.getAttribute(attribute.name) !== attribute.value) {
      oldNode.setAttribute(attribute.name, attribute.value);
    }
  }
  for (const attribute of Array.from(oldNode.attributes)) {
    if (newNode.hasAttribute(attribute.name)) continue;
    // A <details> opened by the reader (or by navigate-to-claim) must not
    // snap shut because a render regenerated it closed; the open state is
    // the reader's, not the renderer's.
    if (oldNode.tagName === "DETAILS" && attribute.name === "open") continue;
    oldNode.removeAttribute(attribute.name);
  }
}

// Form controls carry live state (dirty value, checkedness) that attribute
// sync alone cannot express. Push the rendered value only when it actually
// differs from the control's live value: while the operator types, state and
// DOM already agree, so the control — and its undo stack — is left untouched.
// A real difference means the state changed underneath the control (a loaded
// note, a cleared draft, a meeting switch), where resetting undo is correct.
// Returns true when the node's children are fully handled here.
function syncFormState(oldNode, newNode) {
  if (oldNode.tagName === "TEXTAREA") {
    const incoming = newNode.value;
    if (oldNode.value !== incoming) {
      oldNode.textContent = incoming;
      oldNode.value = incoming;
    } else if (oldNode.textContent !== newNode.textContent) {
      oldNode.textContent = newNode.textContent;
    }
    return true;
  }
  if (oldNode.tagName === "INPUT") {
    if (newNode.type === "checkbox" || newNode.type === "radio") {
      if (oldNode.checked !== newNode.checked) oldNode.checked = newNode.checked;
    } else if (oldNode.value !== newNode.value) {
      oldNode.value = newNode.value;
    }
    return true;
  }
  return false;
}

function morphNode(oldNode, newNode) {
  if (oldNode.nodeType !== Node.ELEMENT_NODE) {
    if (oldNode.nodeValue !== newNode.nodeValue) oldNode.nodeValue = newNode.nodeValue;
    return;
  }
  syncAttributes(oldNode, newNode);
  if (syncFormState(oldNode, newNode)) return;
  patchChildren(oldNode, newNode);
  if (oldNode.tagName === "SELECT" && oldNode.value !== newNode.value) {
    oldNode.value = newNode.value;
  }
}

function patchChildren(oldParent, newParent) {
  const keyed = new Map();
  for (let child = oldParent.firstChild; child; child = child.nextSibling) {
    const key = nodeKey(child);
    if (key && !keyed.has(key)) keyed.set(key, child);
  }
  let oldChild = oldParent.firstChild;
  let newChild = newParent.firstChild;
  while (newChild) {
    const nextNew = newChild.nextSibling;
    const newKey = nodeKey(newChild);
    const keyedMatch = newKey ? keyed.get(newKey) : undefined;
    if (keyedMatch && keyedMatch.tagName === newChild.tagName) {
      keyed.delete(newKey);
      if (keyedMatch === oldChild) {
        oldChild = oldChild.nextSibling;
      } else {
        // Out-of-position keyed node: reordering detaches it (and WebKit may
        // drop its undo stack with it), but that only happens when the
        // structure genuinely moved the editor — staying put is the norm.
        oldParent.insertBefore(keyedMatch, oldChild);
      }
      morphNode(keyedMatch, newChild);
    } else if (oldChild && !nodeKey(oldChild) && compatible(oldChild, newChild)) {
      const matched = oldChild;
      oldChild = oldChild.nextSibling;
      morphNode(matched, newChild);
    } else {
      // No reusable node here: adopt the new one in place and leave oldChild
      // to match a later sibling (an inserted button must not consume the
      // status span that follows it). Unmatched leftovers are removed below.
      oldParent.insertBefore(newChild, oldChild);
    }
    newChild = nextNew;
  }
  while (oldChild) {
    const next = oldChild.nextSibling;
    oldChild.remove();
    oldChild = next;
  }
}

export function patchInto(root, html) {
  const template = document.createElement("template");
  template.innerHTML = html;
  patchChildren(root, template.content);
}
