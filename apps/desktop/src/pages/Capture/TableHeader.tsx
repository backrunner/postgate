import { useCallback, useRef, useState } from "react";
import { useColumnsStore, ColumnConfig } from "@/stores/columns";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuCheckboxItem,
  ContextMenuTrigger,
  ContextMenuSeparator,
  ContextMenuItem,
} from "@/components/ui/context-menu";

interface TableHeaderProps {
  height: number;
}

export function TableHeader({ height }: TableHeaderProps) {
  const columns = useColumnsStore((state) => state.columns);
  const setColumnWidth = useColumnsStore((state) => state.setColumnWidth);
  const setColumnVisible = useColumnsStore((state) => state.setColumnVisible);
  const resetColumns = useColumnsStore((state) => state.resetColumns);

  const visibleColumns = columns.filter((col) => col.visible);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className="flex items-center border-b bg-muted/50 text-xs font-medium text-muted-foreground select-none"
          style={{ height }}
        >
          {visibleColumns.map((col, index) => (
            <HeaderCell
              key={col.id}
              column={col}
              isLast={index === visibleColumns.length - 1}
              onResize={(width) => setColumnWidth(col.id, width)}
            />
          ))}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground">
          Show Columns
        </div>
        <ContextMenuSeparator />
        {columns.map((col) => (
          <ContextMenuCheckboxItem
            key={col.id}
            checked={col.visible}
            onCheckedChange={(checked) => setColumnVisible(col.id, checked)}
          >
            {col.label}
          </ContextMenuCheckboxItem>
        ))}
        <ContextMenuSeparator />
        <ContextMenuItem onClick={resetColumns}>
          Reset to Default
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

interface HeaderCellProps {
  column: ColumnConfig;
  isLast: boolean;
  onResize: (width: number) => void;
}

function HeaderCell({ column, isLast, onResize }: HeaderCellProps) {
  const [isDragging, setIsDragging] = useState(false);
  const cellRef = useRef<HTMLDivElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!column.resizable) return;
      e.preventDefault();
      e.stopPropagation();

      setIsDragging(true);
      startXRef.current = e.clientX;
      startWidthRef.current = cellRef.current?.offsetWidth ?? column.width;

      const handleMouseMove = (e: MouseEvent) => {
        const delta = e.clientX - startXRef.current;
        const newWidth = Math.max(column.minWidth, startWidthRef.current + delta);
        onResize(newWidth);
      };

      const handleMouseUp = () => {
        setIsDragging(false);
        document.removeEventListener("mousemove", handleMouseMove);
        document.removeEventListener("mouseup", handleMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [column, onResize]
  );

  // flex column (path)
  const isFlex = column.width === 0;
  const style: React.CSSProperties = isFlex
    ? { flex: 1, minWidth: column.minWidth }
    : { width: column.width, flexShrink: 0 };

  // Alignment based on column type
  const alignClass =
    column.id === "duration" || column.id === "size"
      ? "text-right pr-2"
      : column.id === "method"
      ? "pl-2"
      : "";

  return (
    <div
      ref={cellRef}
      className={`relative flex items-center truncate ${alignClass}`}
      style={style}
    >
      <span className="truncate">{column.label}</span>
      {column.resizable && !isLast && !isFlex && (
        <div
          className={`absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-primary/50 ${
            isDragging ? "bg-primary" : ""
          }`}
          onMouseDown={handleMouseDown}
        />
      )}
    </div>
  );
}
