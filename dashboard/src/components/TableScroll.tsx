import type { ReactNode } from 'react';

interface TableScrollProps {
  children: ReactNode;
}

export default function TableScroll({ children }: TableScrollProps) {
  return <div className="table-scroll scrollbar">{children}</div>;
}
