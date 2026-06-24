package com.dbx.agent;

import java.util.Objects;

public final class TableInfo {
    private String name;
    private String table_type;
    private String comment;

    public TableInfo() {
        this("", "", null);
    }

    public TableInfo(String name, String table_type) {
        this(name, table_type, null);
    }

    public TableInfo(String name, String table_type, String comment) {
        this.name = name;
        this.table_type = table_type;
        this.comment = comment;
    }

    public String getName() {
        return name;
    }

    public String getTable_type() {
        return table_type;
    }

    public String getComment() {
        return comment;
    }

    public void setName(String name) {
        this.name = name;
    }

    public void setTable_type(String table_type) {
        this.table_type = table_type;
    }

    public void setComment(String comment) {
        this.comment = comment;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) return true;
        if (!(other instanceof TableInfo)) return false;
        TableInfo that = (TableInfo) other;
        return Objects.equals(name, that.name)
            && Objects.equals(table_type, that.table_type)
            && Objects.equals(comment, that.comment);
    }

    @Override
    public int hashCode() {
        return Objects.hash(name, table_type, comment);
    }

    @Override
    public String toString() {
        return "TableInfo(name=" + name + ", table_type=" + table_type + ", comment=" + comment + ")";
    }
}
