import { defineConfig, presetWind3 } from 'unocss'

export default defineConfig({
  presets: [presetWind3()],
  theme: {
    colors: {
      tx: {
        accent: 'var(--tx-accent)',
        text: 'var(--tx-text-primary)',
        muted: 'var(--tx-text-secondary)',
        border: 'var(--tx-border-subtle)',
        surface: 'var(--tx-bg-surface)',
      },
    },
  },
  shortcuts: {
    'tx-focus': 'outline-none ring-2 ring-tx-accent/25',
    'tx-focus-inset': 'outline-none shadow-[inset_0_0_0_1px_var(--tx-border-hover)]',
    'tx-row': 'flex items-center min-w-0',
    'tx-icon-btn': [
      'w-28px min-w-28px h-28px flex-none box-border border-0 rounded-[var(--tx-radius-sm)]',
      'bg-transparent p-0 inline-flex items-center justify-center cursor-pointer',
      'text-[var(--tx-text-secondary)] transition-colors transition-shadow transition-opacity duration-[120ms]',
      'hover:bg-[var(--tx-bg-hover)] hover:text-[var(--tx-text-primary)]',
      'focus:outline-none focus-visible:bg-[var(--tx-bg-hover)] focus-visible:shadow-[inset_0_0_0_1px_var(--tx-border-hover)]',
      'disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent',
    ].join(' '),
    'tx-icon-btn-primary': 'text-[var(--tx-accent)] hover:text-[var(--tx-accent)] focus-visible:text-[var(--tx-accent)]',
    'tx-icon-btn-danger': 'text-[var(--tx-danger)] hover:text-[var(--tx-danger)] focus-visible:text-[var(--tx-danger)]',
    'tx-tooltip': 'relative min-w-0 inline-flex items-center overflow-visible',
    'tx-tooltip-block': 'flex w-fit max-w-full',
    'tx-tooltip-bubble': [
      'absolute z-20 w-max max-w-[240px] border border-white/8 rounded-[5px]',
      'bg-[rgba(23,26,33,0.96)] text-[#f8fafc] shadow-[0_10px_24px_rgba(15,23,42,0.18),0_1px_2px_rgba(15,23,42,0.2)]',
      'px-[7px] py-[5px] font-sans text-[11px] leading-[1.3] whitespace-normal pointer-events-none',
      'opacity-0 translate-y-[2px] transition duration-[120ms]',
    ].join(' '),
  },
})
