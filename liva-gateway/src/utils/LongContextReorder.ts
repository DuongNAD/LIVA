export function longContextReorder<T>(items: T[]): T[] {
    if (!items || items.length === 0) return items;
    
    const left: T[] = [];
    const right: T[] = [];
    
    for (let i = 0; i < items.length; i++) {
        if (i % 2 === 0) {
            left.push(items[i]);
        } else {
            right.unshift(items[i]);
        }
    }
    
    return [...left, ...right];
}
