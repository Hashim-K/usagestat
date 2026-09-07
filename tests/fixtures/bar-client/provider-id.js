export function providerBaseId(provider) {
    if (typeof provider === 'string')
        return providerAlias(provider);
    return providerAlias(provider?.id || '');
}

export function providerAlias(id) {
    return {
        opencodego: 'opencode-go',
        kimik2: 'kimi-k2',
        jetbrains: 'jetbrains-ai-assistant',
        'z-ai': 'zai',
    }[id] || id;
}

