import { HTMLMotionProps, Variants } from 'framer-motion';

/**
 * Standard reusable Framer Motion configurations for Cyberpunk-Executive UI.
 * Consistently applies premium easing, micro-scales, and subtle fades.
 */

// 1. Subtle, ultra-smooth spring transitions
export const PREMIUM_SPRING_TRANSITION = {
    type: 'spring',
    stiffness: 380,
    damping: 30,
    mass: 0.8,
};

// 2. Focused hover & tap scaling properties (for cards and action buttons)
export const PREMIUM_INTERACTIVE_HOVER: HTMLMotionProps<'div'> = {
    whileHover: { scale: 1.015, y: -2 },
    whileTap: { scale: 0.985 },
    transition: PREMIUM_SPRING_TRANSITION,
};

// 3. Fading and sliding variants for modal views or sidebar drawers
export const FADE_IN_UP_VARIANTS: Variants = {
    hidden: {
        opacity: 0,
        y: 15,
    },
    visible: {
        opacity: 1,
        y: 0,
        transition: {
            duration: 0.25,
            ease: [0.16, 1, 0.3, 1], // Custom ease-out cubic
        },
    },
    exit: {
        opacity: 0,
        y: -10,
        transition: {
            duration: 0.15,
            ease: 'easeIn',
        },
    },
};
