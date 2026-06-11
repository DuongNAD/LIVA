import { ref, type Ref } from "vue";

export function useWidgetDrag(isCollapsed: Ref<boolean>) {
  const dragOffset = ref({ x: 0, y: 0 });
  const isDragging = ref(false);
  const snapPosition = ref("right");
  const verticalSnapPosition = ref("bottom");

  let startMousePos = { x: 0, y: 0 };
  let startDragOffset = { x: 0, y: 0 };

  const snapToEdge = () => {
    const collapsedWidth = 48; // w-12 is 48px
    const naturalLeft = window.innerWidth - 16 - collapsedWidth;
    const currentCenterX = naturalLeft + dragOffset.value.x + collapsedWidth / 2;

    if (currentCenterX < window.innerWidth / 2) {
      snapPosition.value = "left";
      dragOffset.value.x = 16 - naturalLeft;
    } else {
      snapPosition.value = "right";
      dragOffset.value.x = 0;
    }
  };

  const onDragMove = (e: MouseEvent) => {
    if (!isDragging.value) return;
    const nextX = startDragOffset.x + (e.clientX - startMousePos.x);
    const nextY = startDragOffset.y + (e.clientY - startMousePos.y);
    const maxX = Math.max(window.innerWidth - 120, 0);
    const maxY = Math.max(window.innerHeight - 120, 0);
    dragOffset.value = {
      x: Math.min(Math.max(nextX, -maxX), maxX),
      y: Math.min(Math.max(nextY, -maxY), maxY),
    };
  };

  const onDragEnd = () => {
    isDragging.value = false;
    globalThis.document.removeEventListener("mousemove", onDragMove);
    globalThis.document.removeEventListener("mouseup", onDragEnd);

    const currentWidth = isCollapsed.value ? 48 : 400;
    const naturalLeft = window.innerWidth - 16 - currentWidth;
    const currentCenterX = naturalLeft + dragOffset.value.x + currentWidth / 2;
    snapPosition.value = currentCenterX < window.innerWidth / 2 ? "left" : "right";

    const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
    verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? "top" : "bottom";

    if (isCollapsed.value) {
      snapToEdge();
    }
  };

  const onDragStart = (e: MouseEvent) => {
    isDragging.value = true;
    startMousePos = { x: e.clientX, y: e.clientY };
    startDragOffset = { ...dragOffset.value };
    globalThis.document.addEventListener("mousemove", onDragMove);
    globalThis.document.addEventListener("mouseup", onDragEnd);
  };

  return {
    dragOffset,
    isDragging,
    snapPosition,
    verticalSnapPosition,
    onDragStart,
    snapToEdge,
  };
}
