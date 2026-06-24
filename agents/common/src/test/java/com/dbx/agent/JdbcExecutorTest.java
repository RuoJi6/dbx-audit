package com.dbx.agent;

import org.junit.jupiter.api.Test;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.sql.ResultSet;
import java.sql.Types;
import javax.sql.rowset.serial.SerialBlob;

import static org.junit.jupiter.api.Assertions.assertEquals;

class JdbcExecutorTest {
    @Test
    void stringResultValueFormatsBlobWithoutUsingStringConversion() throws Exception {
        ResultSet rs = resultSet(
            new byte[]{0x01, 0x2A, (byte) 0xFF},
            () -> {
                throw new AssertionError("BLOB columns should not be read with getString");
            }
        );

        assertEquals("0x012aff", JdbcExecutor.stringResultValue(rs, 1, Types.BLOB));
    }

    @Test
    void defaultResultValueReadsClobColumnsAsText() throws Exception {
        ResultSet rs = resultSet(null, () -> "hello clob");

        assertEquals("hello clob", JdbcExecutor.INSTANCE.defaultResultValue(rs, 1, Types.CLOB));
    }

    @Test
    void defaultResultValueNormalizesBlobObjectsFromFallbackTypes() throws Exception {
        ResultSet rs = resultSet(new SerialBlob(new byte[]{0x0A, 0x0B}), null, false);

        assertEquals("0x0a0b", JdbcExecutor.INSTANCE.defaultResultValue(rs, 1, Types.OTHER));
    }

    private static ResultSet resultSet(byte[] bytes, StringSupplier stringSupplier) {
        return resultSet(bytes, stringSupplier, false);
    }

    private static ResultSet resultSet(Object objectValue, StringSupplier stringSupplier, boolean wasNull) {
        InvocationHandler handler = (Object unused, Method method, Object[] args) -> {
            switch (method.getName()) {
                case "getObject":
                    return objectValue;
                case "getBytes":
                    return objectValue instanceof byte[] ? objectValue : null;
                case "getString":
                    return stringSupplier == null ? null : stringSupplier.get();
                case "wasNull":
                    return wasNull;
                default:
                    return defaultValue(method.getReturnType());
            }
        };
        return (ResultSet) Proxy.newProxyInstance(
            ResultSet.class.getClassLoader(),
            new Class<?>[]{ResultSet.class},
            handler
        );
    }

    private static Object defaultValue(Class<?> type) {
        if (type == Boolean.TYPE) {
            return false;
        }
        if (type == Byte.TYPE) {
            return (byte) 0;
        }
        if (type == Short.TYPE) {
            return (short) 0;
        }
        if (type == Integer.TYPE) {
            return 0;
        }
        if (type == Long.TYPE) {
            return 0L;
        }
        if (type == Float.TYPE) {
            return 0f;
        }
        if (type == Double.TYPE) {
            return 0.0d;
        }
        if (type == Character.TYPE) {
            return '\0';
        }
        return null;
    }

    private interface StringSupplier {
        String get() throws Exception;
    }
}
