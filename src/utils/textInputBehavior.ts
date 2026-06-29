const NON_TEXT_INPUT_TYPES = new Set([
  'button',
  'checkbox',
  'color',
  'file',
  'hidden',
  'image',
  'radio',
  'range',
  'reset',
  'submit',
])

function isTextEntryElement(element: Element): element is HTMLInputElement | HTMLTextAreaElement {
  if (element instanceof HTMLTextAreaElement) return true
  if (!(element instanceof HTMLInputElement)) return false
  return !NON_TEXT_INPUT_TYPES.has(element.type)
}

function disableInputAssists(element: Element) {
  if (!isTextEntryElement(element)) return
  element.autocapitalize = 'off'
  element.autocomplete = 'off'
  element.spellcheck = false
  element.setAttribute('autocorrect', 'off')
}

function disableInputAssistsIn(root: ParentNode) {
  if (root instanceof Element) {
    disableInputAssists(root)
  }
  root.querySelectorAll('input, textarea').forEach(disableInputAssists)
}

export function installPlainTextInputBehavior() {
  if (typeof document === 'undefined') return
  disableInputAssistsIn(document)
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node instanceof Element) {
          disableInputAssistsIn(node)
        }
      }
    }
  })
  observer.observe(document.documentElement, { childList: true, subtree: true })
}
