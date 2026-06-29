import type { LanguageFn } from 'highlight.js'

const KV_VALUE_RE = /"([^"\\]|\\.)*"|[^\s]+/

export const tunnelLogLanguage: LanguageFn = () => ({
    name: 'tunnel-log',
    aliases: ['tunnel-log'],
    contains: [
        {
            scope: 'number',
            begin: /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?/,
        },
        {
            scope: 'symbol',
            begin: /\[(?:T|D|I|W|E|F)\]/,
        },
        {
            scope: 'section',
            begin: /\[[^\]\r\n]+\]/,
        },
        {
            scope: 'keyword',
            begin: /\b(?:access|self-check|start|stop|reload|open|close|listen|connect|disconnect|connected|error|warning|failed|success)\b/i,
        },
        {
            scope: 'type',
            begin: /\b(?:TCP|UDP|HTTP|HTTPS|STCP|SUDP|XTCP|TCPMUX|TLS|KCP|QUIC|WEBSOCKET|WSS)\b/,
        },
        {
            begin: /\b([A-Za-z][\w.-]*)(=)/,
            beginScope: {
                1: 'attr',
                2: 'operator',
            },
            end: /(?=\s|$)/,
            contains: [
                {
                    scope: 'string',
                    begin: KV_VALUE_RE,
                },
            ],
        },
    ],
})
