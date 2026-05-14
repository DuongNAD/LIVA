declare module 'qrcode' {
    interface QRCodeOptions {
        type?: string;
        width?: number;
        margin?: number;
        color?: { dark?: string; light?: string };
    }
    export function toFile(path: string, data: string, options?: QRCodeOptions): Promise<void>;
    export function toDataURL(data: string, options?: QRCodeOptions): Promise<string>;
    export function toString(data: string, options?: QRCodeOptions): Promise<string>;
}
